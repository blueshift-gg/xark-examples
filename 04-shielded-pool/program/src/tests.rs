//! The program's account checks and public inputs, on the host. Each test builds
//! the accounts as the runtime lays them out and calls the entrypoint's function.
//! The host has no runtime: a CPI does nothing and the Rent sysvar is unsupported,
//! so an instruction that passes every check and asks for rent, to create an
//! account, ends in `UnsupportedSysvar` ([`reaches_creation`]). The client test runs
//! the same instructions end to end in LiteSVM.
use super::*;
use pinocchio::account::{NOT_BORROWED, RuntimeAccount};
use shielded_pool_deposit_xark_verifier as deposit_verifier;
use shielded_pool_deposit_xark_verifier::Proof; // the same type in both verifier crates
use shielded_pool_withdraw_xark_verifier as withdraw_verifier;

const PROGRAM: Address = ID;
const SYSTEM: Address = pinocchio_system::ID;
const DENOMINATION: u64 = 1_000_000_000;

/// The demo proofs (`circuits/*/examples/demo.rs`) and their public inputs.
const DEPOSIT_PROOF: &[u8; 256] =
    include_bytes!("../../circuits/deposit/shielded_pool_deposit-verifier/proof.solana.bin");
const DEPOSIT_INPUTS: &[u8; 128] = include_bytes!(
    "../../circuits/deposit/shielded_pool_deposit-verifier/public_inputs.solana.bin"
);
const WITHDRAW_PROOF: &[u8; 256] =
    include_bytes!("../../circuits/withdraw/shielded_pool_withdraw-verifier/proof.solana.bin");
const WITHDRAW_INPUTS: &[u8; 224] = include_bytes!(
    "../../circuits/withdraw/shielded_pool_withdraw-verifier/public_inputs.solana.bin"
);

/// The `i`th 32-byte field element of a public-inputs file.
fn input(inputs: &[u8], i: usize) -> [u8; 32] {
    inputs[32 * i..32 * (i + 1)].try_into().unwrap()
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
                lamports: 1_000_000_000_000,
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

    fn owner(mut self, owner: Address) -> Account {
        self.header().owner = owner;
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
    process_instruction(&PROGRAM, &mut views, data)
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

fn custom(error: ShieldedPoolError) -> ProgramResult {
    Err(ProgramError::Custom(error as u32))
}

// ---- fixtures ------------------------------------------------------------------

fn pool_address(denomination: u64) -> (Address, u8) {
    Address::find_program_address(&[POOL_SEED, &denomination.to_le_bytes()], &PROGRAM)
}

fn nullifier_address(denomination: u64, nullifier_hash: &[u8; 32]) -> Address {
    let seeds: [&[u8]; 3] = [NULLIFIER_SEED, &denomination.to_le_bytes(), nullifier_hash];
    Address::find_program_address(&seeds, &PROGRAM).0
}

/// A pool's bytes: `roots` from the ring's start, the newest last.
fn pool_bytes(denomination: u64, bump: u8, roots: &[[u8; 32]], next_index: u32) -> Vec<u8> {
    let mut data = vec![0u8; Pool::LEN];
    data[0] = Pool::TAG;
    data[1] = bump;
    data[8..16].copy_from_slice(&denomination.to_le_bytes());
    data[16..20].copy_from_slice(&next_index.to_le_bytes());
    data[20..24].copy_from_slice(&(roots.len() as u32 - 1).to_le_bytes());
    for (i, root) in roots.iter().enumerate() {
        data[24 + 32 * i..56 + 32 * i].copy_from_slice(root);
    }
    data
}

/// The pool after initialize, or with `roots` and `next_index`.
fn pool(roots: &[[u8; 32]], next_index: u32) -> Account {
    let (address, bump) = pool_address(DENOMINATION);
    Account::new(
        address,
        PROGRAM,
        &pool_bytes(DENOMINATION, bump, roots, next_index),
    )
}

// ---- initialize ----------------------------------------------------------------

fn initialize_accounts() -> [Account; 3] {
    let (address, _) = pool_address(DENOMINATION);
    [
        Account::new(address, SYSTEM, &[]),
        Account::wallet(1),
        Account::system(),
    ]
}

fn initialize_ix() -> Vec<u8> {
    ix(*Initialize::DISCRIMINATOR, &[&DENOMINATION.to_le_bytes()])
}

#[test]
fn initialize_checks_its_accounts() {
    reaches_creation(run(&initialize_ix(), &mut initialize_accounts()));

    let mut accounts = initialize_accounts();
    accounts[1] = Account::wallet(1).signer(false);
    assert_eq!(
        run(&initialize_ix(), &mut accounts),
        Err(ProgramError::MissingRequiredSignature)
    );

    let mut accounts = initialize_accounts();
    accounts[1] = Account::wallet(1).writable(false);
    assert_eq!(
        run(&initialize_ix(), &mut accounts),
        Err(ProgramError::Immutable)
    );

    let mut accounts = initialize_accounts();
    accounts[2] = Account::wallet(3);
    assert_eq!(
        run(&initialize_ix(), &mut accounts),
        Err(ProgramError::IncorrectProgramId)
    );

    let mut accounts = initialize_accounts();
    accounts[0] = Account::new(pool_address(DENOMINATION).0, SYSTEM, &[]).writable(false);
    assert_eq!(
        run(&initialize_ix(), &mut accounts),
        Err(ProgramError::Immutable)
    );

    // Another denomination's address, and a non-canonical bump's.
    let mut accounts = initialize_accounts();
    accounts[0] = Account::new(pool_address(DENOMINATION + 1).0, SYSTEM, &[]);
    assert_eq!(
        run(&initialize_ix(), &mut accounts),
        Err(ProgramError::InvalidSeeds)
    );
    let (_, bump) = pool_address(DENOMINATION);
    let other = (0..bump)
        .rev()
        .find_map(|b| {
            Address::create_program_address(
                &[POOL_SEED, &DENOMINATION.to_le_bytes(), &[b]],
                &PROGRAM,
            )
            .ok()
        })
        .unwrap();
    accounts[0] = Account::new(other, SYSTEM, &[]);
    assert_eq!(
        run(&initialize_ix(), &mut accounts),
        Err(ProgramError::InvalidSeeds)
    );

    // Instruction data of the wrong size, an unknown tag, too few accounts.
    let mut accounts = initialize_accounts();
    assert_eq!(
        run(&initialize_ix()[..8], &mut accounts),
        Err(ProgramError::InvalidInstructionData)
    );
    assert_eq!(
        run(&[7], &mut accounts),
        Err(ProgramError::InvalidInstructionData)
    );
    assert_eq!(
        run(&[], &mut accounts),
        Err(ProgramError::InvalidInstructionData)
    );
    assert_eq!(
        run(&initialize_ix(), &mut accounts[..2]),
        Err(ProgramError::NotEnoughAccountKeys)
    );
}

#[test]
fn initialize_refuses_an_existing_pool() {
    let mut accounts = initialize_accounts();
    accounts[0] = pool(&[EMPTY_ROOT], 0);
    assert_eq!(
        run(&initialize_ix(), &mut accounts),
        Err(ProgramError::AccountAlreadyInitialized)
    );
    // Still system-owned, but holding data.
    accounts[0] = Account::new(pool_address(DENOMINATION).0, SYSTEM, &[0]);
    assert_eq!(
        run(&initialize_ix(), &mut accounts),
        Err(ProgramError::AccountAlreadyInitialized)
    );
    // Lamports sent to the address beforehand don't block it.
    accounts[0] = Account::new(pool_address(DENOMINATION).0, SYSTEM, &[]);
    reaches_creation(run(&initialize_ix(), &mut accounts));
}

// ---- deposit -------------------------------------------------------------------

fn deposit_accounts() -> [Account; 3] {
    [
        pool(&[EMPTY_ROOT], 0),
        Account::wallet(1),
        Account::system(),
    ]
}

/// The demo's deposit: leaf 7 into the empty tree.
fn deposit_ix() -> Vec<u8> {
    let (new_root, leaf) = (input(DEPOSIT_INPUTS, 1), input(DEPOSIT_INPUTS, 2));
    ix(*Deposit::DISCRIMINATOR, &[&leaf, &new_root, DEPOSIT_PROOF])
}

/// The program's empty root and inputs are the demo's, which is the first deposit
/// into an empty tree, at index 0.
#[test]
fn the_deposit_inputs_are_the_circuits() {
    assert_eq!(input(DEPOSIT_INPUTS, 0), EMPTY_ROOT);
    let (new_root, leaf) = (input(DEPOSIT_INPUTS, 1), input(DEPOSIT_INPUTS, 2));
    let inputs = deposit_inputs(&EMPTY_ROOT, &new_root, &leaf, 0);
    assert_eq!(inputs.as_flattened(), DEPOSIT_INPUTS);
    assert!(deposit_verifier::verify(
        &Proof::from_le_bytes(DEPOSIT_PROOF),
        &inputs
    ));
    assert!(!deposit_verifier::verify(
        &Proof::from_le_bytes(DEPOSIT_PROOF),
        &deposit_inputs(&EMPTY_ROOT, &new_root, &leaf, 1)
    ));
}

#[test]
fn a_deposit_advances_the_tree() {
    let mut accounts = deposit_accounts();
    assert_eq!(run(&deposit_ix(), &mut accounts), Ok(()));
    let new_root = input(DEPOSIT_INPUTS, 1);
    let (_, bump) = pool_address(DENOMINATION);
    assert_eq!(
        accounts[0].data(),
        pool_bytes(DENOMINATION, bump, &[EMPTY_ROOT, new_root], 1)
    );
    // The same proof against the advanced tree: its old root is no longer current.
    assert_eq!(
        run(&deposit_ix(), &mut accounts),
        custom(ShieldedPoolError::InvalidProof)
    );
}

#[test]
fn deposit_checks_its_accounts() {
    let check = |i: usize, account: Account, error: ProgramError| {
        let mut accounts = deposit_accounts();
        accounts[i] = account;
        assert_eq!(run(&deposit_ix(), &mut accounts), Err(error));
    };
    let (address, bump) = pool_address(DENOMINATION);
    let bytes = pool_bytes(DENOMINATION, bump, &[EMPTY_ROOT], 0);
    check(
        0,
        pool(&[EMPTY_ROOT], 0).writable(false),
        ProgramError::Immutable,
    );
    check(
        0,
        pool(&[EMPTY_ROOT], 0).owner(SYSTEM),
        ProgramError::InvalidAccountOwner,
    );
    check(
        0,
        Account::new(address, PROGRAM, &bytes[..535]),
        ProgramError::InvalidAccountData,
    );
    // A nullifier's marker, owned by the program, isn't a pool.
    check(
        0,
        Account::new(address, PROGRAM, &[Nullifier::TAG]),
        ProgramError::InvalidAccountData,
    );
    let mut tagged = bytes.clone();
    tagged[0] = Nullifier::TAG;
    check(
        0,
        Account::new(address, PROGRAM, &tagged),
        ProgramError::InvalidAccountData,
    );
    // A pool's bytes at another address, or with another bump.
    check(
        0,
        Account::new(pool_address(5).0, PROGRAM, &bytes),
        ProgramError::InvalidSeeds,
    );
    let other_bump = pool_bytes(DENOMINATION, bump.wrapping_sub(1), &[EMPTY_ROOT], 0);
    check(
        0,
        Account::new(address, PROGRAM, &other_bump),
        ProgramError::InvalidSeeds,
    );

    check(
        1,
        Account::wallet(1).signer(false),
        ProgramError::MissingRequiredSignature,
    );
    check(
        1,
        Account::wallet(1).writable(false),
        ProgramError::Immutable,
    );
    check(2, Account::wallet(3), ProgramError::IncorrectProgramId);
}

#[test]
fn a_full_tree_takes_no_deposit() {
    let mut accounts = deposit_accounts();
    accounts[0] = pool(&[EMPTY_ROOT], 1 << TREE_HEIGHT);
    assert_eq!(
        run(&deposit_ix(), &mut accounts),
        custom(ShieldedPoolError::TreeFull)
    );
}

// ---- withdraw ------------------------------------------------------------------

/// The demo's withdrawal: note (1, 2), the only one in the tree, to recipient
/// `[1; 32]` through relayer `[2; 32]` for no fee.
fn withdraw_accounts() -> [Account; 5] {
    let nullifier_hash = input(WITHDRAW_INPUTS, 1);
    [
        pool(&[EMPTY_ROOT, input(WITHDRAW_INPUTS, 0)], 1),
        Account::new(
            nullifier_address(DENOMINATION, &nullifier_hash),
            SYSTEM,
            &[],
        ),
        Account::new(Address::new_from_array([1; 32]), SYSTEM, &[]),
        Account::wallet(2),
        Account::system(),
    ]
}

fn withdraw_ix(root: &[u8; 32], fee: u64) -> Vec<u8> {
    let nullifier_hash = input(WITHDRAW_INPUTS, 1);
    ix(
        *Withdraw::DISCRIMINATOR,
        &[WITHDRAW_PROOF, root, &nullifier_hash, &fee.to_le_bytes()],
    )
}

fn demo_withdraw_ix() -> Vec<u8> {
    withdraw_ix(&input(WITHDRAW_INPUTS, 0), 0)
}

#[test]
fn the_withdraw_inputs_are_the_circuits() {
    let (root, nullifier_hash) = (input(WITHDRAW_INPUTS, 0), input(WITHDRAW_INPUTS, 1));
    let (recipient, relayer) = (
        Address::new_from_array([1; 32]),
        Address::new_from_array([2; 32]),
    );
    let inputs = withdraw_inputs(&root, &nullifier_hash, &recipient, &relayer, 0);
    assert_eq!(inputs.as_flattened(), WITHDRAW_INPUTS);
    let proof = Proof::from_le_bytes(WITHDRAW_PROOF);
    assert!(withdraw_verifier::verify(&proof, &inputs));
    // The proof is bound to its recipient, relayer and fee.
    let swapped = withdraw_inputs(&root, &nullifier_hash, &relayer, &recipient, 0);
    assert!(!withdraw_verifier::verify(&proof, &swapped));
    let fee = withdraw_inputs(&root, &nullifier_hash, &recipient, &relayer, 1);
    assert!(!withdraw_verifier::verify(&proof, &fee));
}

#[test]
fn a_withdrawal_spends_its_nullifier() {
    reaches_creation(run(&demo_withdraw_ix(), &mut withdraw_accounts()));
    // The marker exists: the note is spent.
    let mut accounts = withdraw_accounts();
    let nullifier = nullifier_address(DENOMINATION, &input(WITHDRAW_INPUTS, 1));
    accounts[1] = Account::new(nullifier, PROGRAM, &[Nullifier::TAG]);
    assert_eq!(
        run(&demo_withdraw_ix(), &mut accounts),
        custom(ShieldedPoolError::NullifierSpent)
    );
    // System-owned but holding data, or lamports only (which don't block it).
    accounts[1] = Account::new(nullifier, SYSTEM, &[0]);
    assert_eq!(
        run(&demo_withdraw_ix(), &mut accounts),
        custom(ShieldedPoolError::NullifierSpent)
    );
    accounts[1] = Account::new(nullifier, SYSTEM, &[]);
    reaches_creation(run(&demo_withdraw_ix(), &mut accounts));
}

#[test]
fn withdraw_checks_the_statement() {
    let root = input(WITHDRAW_INPUTS, 0);
    let mut accounts = withdraw_accounts();
    assert_eq!(
        run(&withdraw_ix(&root, 1), &mut accounts),
        custom(ShieldedPoolError::InvalidProof)
    );
    let too_high = withdraw_ix(&root, DENOMINATION + 1);
    assert_eq!(
        run(&too_high, &mut accounts),
        custom(ShieldedPoolError::FeeTooHigh)
    );
    assert_eq!(
        run(&withdraw_ix(&[9; 32], 0), &mut accounts),
        custom(ShieldedPoolError::UnknownRoot)
    );
    // An unused ring slot is all zeros, and never a root.
    assert_eq!(
        run(&withdraw_ix(&[0; 32], 0), &mut accounts),
        custom(ShieldedPoolError::UnknownRoot)
    );
    // Another recipient, or relayer.
    accounts[2] = Account::new(Address::new_from_array([3; 32]), SYSTEM, &[]);
    assert_eq!(
        run(&demo_withdraw_ix(), &mut accounts),
        custom(ShieldedPoolError::InvalidProof)
    );
    let mut accounts = withdraw_accounts();
    accounts[3] = Account::wallet(3);
    assert_eq!(
        run(&demo_withdraw_ix(), &mut accounts),
        custom(ShieldedPoolError::InvalidProof)
    );
    // A root that has left the ring: sixteen deposits since.
    let mut accounts = withdraw_accounts();
    accounts[0] = pool(&[[5; 32]; ROOT_HISTORY_SIZE], 17);
    assert_eq!(
        run(&demo_withdraw_ix(), &mut accounts),
        custom(ShieldedPoolError::UnknownRoot)
    );
}

#[test]
fn withdraw_checks_its_accounts() {
    let check = |i: usize, account: Account, error: ProgramError| {
        let mut accounts = withdraw_accounts();
        accounts[i] = account;
        assert_eq!(run(&demo_withdraw_ix(), &mut accounts), Err(error));
    };
    let roots = [EMPTY_ROOT, input(WITHDRAW_INPUTS, 0)];
    check(0, pool(&roots, 1).writable(false), ProgramError::Immutable);
    check(
        0,
        pool(&roots, 1).owner(SYSTEM),
        ProgramError::InvalidAccountOwner,
    );
    let (_, bump) = pool_address(DENOMINATION);
    let bytes = pool_bytes(DENOMINATION, bump, &roots, 1);
    check(
        0,
        Account::new(pool_address(5).0, PROGRAM, &bytes),
        ProgramError::InvalidSeeds,
    );

    let nullifier = nullifier_address(DENOMINATION, &input(WITHDRAW_INPUTS, 1));
    check(
        1,
        Account::new(nullifier, SYSTEM, &[]).writable(false),
        ProgramError::Immutable,
    );
    // Another denomination's, or another nullifier's, marker address.
    let elsewhere = nullifier_address(5, &input(WITHDRAW_INPUTS, 1));
    check(
        1,
        Account::new(elsewhere, SYSTEM, &[]),
        ProgramError::InvalidSeeds,
    );
    let another = nullifier_address(DENOMINATION, &[1; 32]);
    check(
        1,
        Account::new(another, SYSTEM, &[]),
        ProgramError::InvalidSeeds,
    );

    let recipient = Address::new_from_array([1; 32]);
    check(
        2,
        Account::new(recipient, SYSTEM, &[]).writable(false),
        ProgramError::Immutable,
    );
    // The pool's own accounts can't receive the payout.
    check(2, pool(&roots, 1), ProgramError::InvalidArgument);
    check(
        2,
        Account::new(nullifier, SYSTEM, &[]),
        ProgramError::InvalidArgument,
    );

    check(
        3,
        Account::wallet(2).signer(false),
        ProgramError::MissingRequiredSignature,
    );
    check(
        3,
        Account::wallet(2).writable(false),
        ProgramError::Immutable,
    );
    check(4, Account::wallet(4), ProgramError::IncorrectProgramId);
}

/// The payout's lamport moves are checked, and each reads the balance it changes.
#[test]
fn the_payout_is_checked_arithmetic() {
    let mut pool = Account::wallet(1);
    pool.header().lamports = DENOMINATION - 1;
    assert_eq!(
        debit(&mut pool.view(), DENOMINATION),
        Err(ProgramError::InsufficientFunds)
    );
    assert_eq!(pool.header().lamports, DENOMINATION - 1);
    let mut full = Account::wallet(2);
    full.header().lamports = u64::MAX;
    assert_eq!(
        credit(&mut full.view(), 1),
        Err(ProgramError::ArithmeticOverflow)
    );
    // A recipient that is also the relayer: two views of one account.
    let mut both = Account::wallet(3);
    both.header().lamports = 0;
    let (mut recipient, mut relayer) = (both.view(), both.view());
    credit(&mut recipient, DENOMINATION - 10).unwrap();
    credit(&mut relayer, 10).unwrap();
    assert_eq!(both.header().lamports, DENOMINATION);
}

/// Each error logs its own name: the client tests match them.
#[test]
fn errors_log_their_names() {
    use ShieldedPoolError::*;
    for (error, name) in [
        (InvalidProof, "InvalidProof"),
        (UnknownRoot, "UnknownRoot"),
        (FeeTooHigh, "FeeTooHigh"),
        (TreeFull, "TreeFull"),
        (NullifierSpent, "NullifierSpent"),
    ] {
        let logged = ProgramError::from(error).to_str::<ShieldedPoolError>();
        assert!(logged.starts_with(&format!("{name}: ")));
    }
    assert_eq!(
        ProgramError::Custom(5).to_str::<ShieldedPoolError>(),
        "Error: Unknown"
    );
}
