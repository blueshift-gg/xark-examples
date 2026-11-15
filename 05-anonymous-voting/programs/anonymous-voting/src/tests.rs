//! The program's account checks and public inputs, on the host. Each test builds
//! the accounts as the runtime lays them out and calls the entrypoint's function.
//! The host has no runtime: a CPI does nothing and the Rent sysvar is unsupported,
//! so an instruction that passes every check and asks for rent, to create an
//! account, ends in `UnsupportedSysvar` ([`reaches_creation`]). The client test runs
//! the same instructions end to end in LiteSVM.
use pinocchio::account::{NOT_BORROWED, RuntimeAccount};
use vote_verifier::{Ballot, Fields, Proof, PublicInputs, fields, verify};

use super::*;

const SYSTEM: Address = pinocchio_system::ID;

/// The demo proof (`circuits/vote/examples/demo.rs`: a list of one voter, voting
/// for option 1 of 3 in poll 7) and its public inputs, as the verifier crate
/// bundles them.
const PROOF: &[u8; 256] = include_bytes!("../../../clients/vote/rust/proof.solana.bin");
const INPUTS: &[u8; 128] = include_bytes!("../../../clients/vote/rust/public_inputs.solana.bin");
const POLL_ID: u64 = 7;
const OPTIONS: u8 = 3;
const CHOICE: u8 = 1;

/// The `i`th 32-byte field element of the public inputs.
fn input(i: usize) -> [u8; 32] {
    INPUTS[32 * i..32 * (i + 1)].try_into().unwrap()
}

/// An account as the runtime serializes it: the header, then the data.
struct Account {
    words: Vec<u64>, // 8-aligned, as the runtime's input region is
}

impl Account {
    fn new(address: Address, owner: Address, data: &[u8]) -> Account {
        let header = size_of::<RuntimeAccount>();
        let mut words = vec![0u64; (header + data.len()).div_ceil(8)];
        let raw = words.as_mut_ptr().cast::<RuntimeAccount>();
        // SAFETY: `words` holds a header and `data.len()` bytes after it.
        unsafe {
            raw.write(RuntimeAccount {
                borrow_state: NOT_BORROWED,
                is_signer: 0,
                is_writable: 1,
                address,
                owner,
                lamports: 1_000_000_000,
                data_len: data.len() as u64,
                ..RuntimeAccount::default()
            });
            core::ptr::copy_nonoverlapping(data.as_ptr(), raw.add(1).cast::<u8>(), data.len());
        }
        Account { words }
    }

    /// A system-owned wallet: a signer, writable.
    fn wallet(seed: u8) -> Account {
        Account::new(Address::new_from_array([seed; 32]), SYSTEM, &[]).signer(true)
    }

    fn system() -> Account {
        Account::new(SYSTEM, Address::new_from_array([0xff; 32]), &[]).writable(false)
    }

    fn header(&mut self) -> &mut RuntimeAccount {
        // SAFETY: `words` starts with a header.
        unsafe { &mut *self.words.as_mut_ptr().cast::<RuntimeAccount>() }
    }

    fn signer(mut self, yes: bool) -> Account {
        self.header().is_signer = yes as u8;
        self
    }

    fn writable(mut self, yes: bool) -> Account {
        self.header().is_writable = yes as u8;
        self
    }

    fn view(&mut self) -> AccountView {
        // SAFETY: `words` is a header followed by its data.
        unsafe { AccountView::new_unchecked(self.words.as_mut_ptr().cast()) }
    }

    fn data(&mut self) -> Vec<u8> {
        let view = self.view();
        view.try_borrow().unwrap().to_vec()
    }
}

/// Call the program with `data` and `accounts`.
fn run(data: &[u8], accounts: &mut [Account]) -> ProgramResult {
    let mut views: Vec<AccountView> = accounts.iter_mut().map(Account::view).collect();
    process_instruction(&ID, &mut views, data)
}

fn ix(tag: u8, args: &[&[u8]]) -> Vec<u8> {
    let mut out = vec![tag];
    for arg in args {
        out.extend_from_slice(arg);
    }
    out
}

/// Every check passed, and the program asked the runtime for rent to create an
/// account.
fn reaches_creation(result: ProgramResult) {
    assert_eq!(result, Err(ProgramError::UnsupportedSysvar));
}

fn custom(error: VotingError) -> ProgramResult {
    Err(ProgramError::Custom(error as u32))
}

// ---- fixtures ------------------------------------------------------------------

fn poll_address(id: u64) -> (Address, u8) {
    Address::find_program_address(&[POLL_SEED, &id.to_le_bytes()], &ID)
}

fn receipt_address(poll: &Address, nullifier: &[u8; 32]) -> Address {
    Address::find_program_address(&[VOTED_SEED, poll.as_ref(), nullifier], &ID).0
}

/// The demo's poll, as `create_poll` writes it, with its authority `[1; 32]`.
fn poll_bytes(open: bool) -> Vec<u8> {
    let mut data = vec![0u8; Poll::LEN];
    data[0] = Poll::TAG;
    data[1] = poll_address(POLL_ID).1;
    data[2] = OPTIONS;
    data[3] = open as u8;
    data[8..16].copy_from_slice(&POLL_ID.to_le_bytes());
    data[16..48].copy_from_slice(&[1; 32]);
    data[48..80].copy_from_slice(&input(0));
    data
}

fn poll(open: bool) -> Account {
    Account::new(poll_address(POLL_ID).0, ID, &poll_bytes(open))
}

// ---- public inputs -------------------------------------------------------------

/// The program assembles the demo's public inputs from the poll and the vote, and
/// the demo proof verifies against them, for its poll, list, nullifier and choice
/// only.
#[test]
fn the_demo_proof_verifies_for_its_ballot_only() {
    let poll = poll_bytes(true);
    let inputs = public_inputs(Poll::load(&poll).unwrap(), &input(2), CHOICE);
    assert_eq!(Fields::from(inputs).as_flattened(), INPUTS);
    let proof = Proof::from_le_bytes(PROOF);
    assert!(verify(&proof, inputs));
    let ok = inputs.ballot;
    for ballot in [
        Ballot { choice: 0, ..ok },
        Ballot { options: 4, ..ok },
        Ballot {
            poll: fields::uint(8),
            ..ok
        },
        Ballot {
            root: [1; 32],
            ..ok
        },
        Ballot {
            nullifier: [1; 32],
            ..ok
        },
    ] {
        assert!(!verify(&proof, PublicInputs { ballot }));
    }
}

// ---- create_poll ---------------------------------------------------------------

fn create_accounts() -> [Account; 3] {
    [
        Account::wallet(1),
        Account::new(poll_address(POLL_ID).0, SYSTEM, &[]),
        Account::system(),
    ]
}

fn create_ix(options: u8) -> Vec<u8> {
    ix(
        *CreatePoll::DISCRIMINATOR,
        &[&POLL_ID.to_le_bytes(), &input(0), &[options]],
    )
}

#[test]
fn create_poll_checks_its_accounts_and_options() {
    reaches_creation(run(&create_ix(OPTIONS), &mut create_accounts()));
    reaches_creation(run(&create_ix(8), &mut create_accounts()));
    for options in [0, 1, 9, 255] {
        assert_eq!(
            run(&create_ix(options), &mut create_accounts()),
            Err(ProgramError::InvalidArgument)
        );
    }

    let check = |i: usize, account: Account, error: ProgramError| {
        let mut accounts = create_accounts();
        accounts[i] = account;
        assert_eq!(run(&create_ix(OPTIONS), &mut accounts), Err(error));
    };
    check(
        0,
        Account::wallet(1).signer(false),
        ProgramError::MissingRequiredSignature,
    );
    check(
        1,
        Account::new(poll_address(POLL_ID + 1).0, SYSTEM, &[]),
        ProgramError::InvalidSeeds,
    );
    check(1, poll(true), ProgramError::AccountAlreadyInitialized);
    check(2, Account::wallet(3), ProgramError::IncorrectProgramId);

    let mut accounts = create_accounts();
    assert_eq!(
        run(&create_ix(OPTIONS)[..40], &mut accounts),
        Err(ProgramError::InvalidInstructionData)
    );
    assert_eq!(
        run(&[7], &mut accounts),
        Err(ProgramError::InvalidInstructionData)
    );
    assert_eq!(
        run(&create_ix(OPTIONS), &mut accounts[..2]),
        Err(ProgramError::NotEnoughAccountKeys)
    );
}

// ---- vote ----------------------------------------------------------------------

/// The demo's vote, relayed by `[2; 32]`.
fn vote_accounts() -> [Account; 4] {
    let poll = poll(true);
    let receipt = receipt_address(&poll_address(POLL_ID).0, &input(2));
    [
        Account::wallet(2),
        poll,
        Account::new(receipt, SYSTEM, &[]),
        Account::system(),
    ]
}

fn vote_ix(choice: u8) -> Vec<u8> {
    ix(*Vote::DISCRIMINATOR, &[PROOF, &input(2), &[choice]])
}

#[test]
fn a_vote_verifies_before_its_receipt() {
    reaches_creation(run(&vote_ix(CHOICE), &mut vote_accounts()));
    // The same proof for another choice, or one out of range.
    for choice in [0, 2, 3, 255] {
        assert_eq!(
            run(&vote_ix(choice), &mut vote_accounts()),
            custom(VotingError::InvalidProof)
        );
    }
    // The receipt exists: the nullifier has voted.
    let mut accounts = vote_accounts();
    accounts[2] = Account::new(
        receipt_address(&poll_address(POLL_ID).0, &input(2)),
        ID,
        &[],
    );
    assert_eq!(
        run(&vote_ix(CHOICE), &mut accounts),
        custom(VotingError::AlreadyVoted)
    );
    // A closed poll counts nothing.
    let mut accounts = vote_accounts();
    accounts[1] = poll(false);
    assert_eq!(
        run(&vote_ix(CHOICE), &mut accounts),
        custom(VotingError::PollClosed)
    );
}

#[test]
fn vote_checks_its_accounts() {
    let check = |i: usize, account: Account, error: ProgramError| {
        let mut accounts = vote_accounts();
        accounts[i] = account;
        assert_eq!(run(&vote_ix(CHOICE), &mut accounts), Err(error));
    };
    check(
        0,
        Account::wallet(2).signer(false),
        ProgramError::MissingRequiredSignature,
    );
    check(1, poll(true).writable(false), ProgramError::Immutable);
    let (address, _) = poll_address(POLL_ID);
    check(
        1,
        Account::new(address, SYSTEM, &poll_bytes(true)),
        ProgramError::InvalidAccountOwner,
    );
    check(
        1,
        Account::new(address, ID, &poll_bytes(true)[..143]),
        ProgramError::InvalidAccountData,
    );
    // A poll's bytes at another poll's address.
    check(
        1,
        Account::new(poll_address(POLL_ID + 1).0, ID, &poll_bytes(true)),
        ProgramError::InvalidSeeds,
    );
    // Another nullifier's receipt, another poll's, and a non-canonical bump's.
    check(
        2,
        Account::new(receipt_address(&address, &[1; 32]), SYSTEM, &[]),
        ProgramError::InvalidSeeds,
    );
    check(
        2,
        Account::new(
            receipt_address(&poll_address(POLL_ID + 1).0, &input(2)),
            SYSTEM,
            &[],
        ),
        ProgramError::InvalidSeeds,
    );
    let nullifier = input(2);
    let (_, bump) = Address::find_program_address(&[VOTED_SEED, address.as_ref(), &nullifier], &ID);
    let other = (0..bump)
        .rev()
        .find_map(|b| {
            Address::create_program_address(&[VOTED_SEED, address.as_ref(), &nullifier, &[b]], &ID)
                .ok()
        })
        .unwrap();
    check(
        2,
        Account::new(other, SYSTEM, &[]),
        ProgramError::InvalidSeeds,
    );
    check(3, Account::wallet(3), ProgramError::IncorrectProgramId);

    let mut accounts = vote_accounts();
    assert_eq!(
        run(&vote_ix(CHOICE)[..289], &mut accounts),
        Err(ProgramError::InvalidInstructionData)
    );
}

// ---- close_poll ----------------------------------------------------------------

#[test]
fn only_the_authority_closes_a_poll() {
    let close = ix(*ClosePoll::DISCRIMINATOR, &[]);
    let mut accounts = [Account::wallet(2), poll(true)];
    assert_eq!(
        run(&close, &mut accounts),
        Err(ProgramError::IncorrectAuthority)
    );
    let mut accounts = [Account::wallet(1).signer(false), poll(true)];
    assert_eq!(
        run(&close, &mut accounts),
        Err(ProgramError::MissingRequiredSignature)
    );
    let mut accounts = [Account::wallet(1), poll(true)];
    assert_eq!(run(&close, &mut accounts), Ok(()));
    assert_eq!(accounts[1].data(), poll_bytes(false));
}
