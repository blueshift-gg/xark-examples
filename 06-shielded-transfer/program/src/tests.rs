//! Host tests: the public inputs the program assembles against the circuit's demo
//! proof, and each account check refusing the accounts it exists to refuse. The
//! checks run before any syscall or CPI, so they run here on fake accounts; the
//! client's end-to-end test covers the CPIs in LiteSVM.
use crate::{ID as PROGRAM, errors::ShieldedTransferError, instructions::*, state::*};
use pinocchio::{AccountView, Address, account::RuntimeAccount, error::ProgramError};
use pinocchio_token::state::{Account as TokenAccount, Mint};
use shielded_transfer_transact_xark_verifier::{NUM_PUBLIC_INPUTS, Proof, verify};
use solana_sha256_hasher::hash;
const MINT: Address = Address::new_from_array([7; 32]);

/// A fake account: its `RuntimeAccount` header with its data after it, as the
/// entrypoint lays them out, kept alive by the buffer.
struct Fake {
    buffer: Vec<u64>,
}

impl Fake {
    fn new(address: Address, owner: Address, signer: bool, writable: bool, data: &[u8]) -> Self {
        let header = core::mem::size_of::<RuntimeAccount>();
        let mut buffer = vec![0u64; (header + data.len()).div_ceil(8) + 1];
        let raw = buffer.as_mut_ptr() as *mut RuntimeAccount;
        // SAFETY: `buffer` is 8-aligned and holds the header and the data.
        unsafe {
            raw.write(RuntimeAccount {
                borrow_state: pinocchio::account::NOT_BORROWED,
                is_signer: signer as u8,
                is_writable: writable as u8,
                executable: 0,
                padding: [0; 4],
                address,
                owner,
                lamports: 1_000_000,
                data_len: data.len() as u64,
            });
            let at = (raw as *mut u8).add(header);
            core::ptr::copy_nonoverlapping(data.as_ptr(), at, data.len());
        }
        Self { buffer }
    }

    fn view(&mut self) -> AccountView {
        // SAFETY: `new` laid out a header and its data.
        unsafe { AccountView::new_unchecked(self.buffer.as_mut_ptr() as *mut RuntimeAccount) }
    }
}

fn pda(seeds: &[&[u8]]) -> (Address, u8) {
    Address::find_program_address(seeds, &PROGRAM)
}

fn system(address: Address) -> Fake {
    Fake::new(address, pinocchio_system::ID, false, true, &[])
}

fn wallet() -> Fake {
    Fake::new(
        Address::new_from_array([1; 32]),
        pinocchio_system::ID,
        true,
        true,
        &[],
    )
}

fn program(id: Address) -> Fake {
    Fake::new(id, Address::new_from_array([2; 32]), false, false, &[])
}

/// An initialized SPL mint's 82 bytes.
fn mint_data() -> Vec<u8> {
    let mut data = vec![0u8; Mint::LEN];
    data[45] = 1; // is_initialized
    data
}

/// An initialized SPL token account's 165 bytes: `mint`, `owner`, `amount`.
fn token_data(mint: &Address, owner: &Address, amount: u64) -> Vec<u8> {
    let mut data = vec![0u8; TokenAccount::LEN];
    data[..32].copy_from_slice(mint.as_ref());
    data[32..64].copy_from_slice(owner.as_ref());
    data[64..72].copy_from_slice(&amount.to_le_bytes());
    data[108] = 1; // state: Initialized
    data
}

fn token(address: Address, mint: &Address, owner: &Address) -> Fake {
    Fake::new(
        address,
        pinocchio_token::ID,
        false,
        true,
        &token_data(mint, owner, 0),
    )
}

/// A refusal case: what it changes in the accounts, and the error it expects.
type Case<'a> = (&'a str, Box<dyn FnOnce(&mut Vec<Fake>) + 'a>, ProgramError);

fn views(fakes: &mut [Fake]) -> Vec<AccountView> {
    fakes.iter_mut().map(Fake::view).collect()
}

// ---- initialize --------------------------------------------------------------------

fn initialize_fakes() -> Vec<Fake> {
    let (pool, _) = pda(&[POOL_SEED, MINT.as_ref()]);
    let (vault, _) = pda(&[VAULT_SEED, MINT.as_ref()]);
    vec![
        system(pool),
        system(vault),
        Fake::new(MINT, pinocchio_token::ID, false, false, &mint_data()),
        wallet(),
        program(pinocchio_token::ID),
        program(pinocchio_system::ID),
    ]
}

fn initialize_with(change: impl FnOnce(&mut Vec<Fake>)) -> Result<(), ProgramError> {
    let mut fakes = initialize_fakes();
    change(&mut fakes);
    InitializeAccounts::try_from(&mut views(&mut fakes)[..]).map(|_| ())
}

#[test]
fn initialize_takes_its_accounts() {
    let mut fakes = initialize_fakes();
    let mut views = views(&mut fakes);
    let a = InitializeAccounts::try_from(&mut views[..]).unwrap();
    assert_eq!(a.pool_bump, pda(&[POOL_SEED, MINT.as_ref()]).1);
    assert_eq!(a.vault_bump, pda(&[VAULT_SEED, MINT.as_ref()]).1);
}

#[test]
fn initialize_refuses_bad_accounts() {
    use ProgramError::*;
    let cases: [Case<'_>; 11] = [
        ("too few", Box::new(|f| drop(f.pop())), NotEnoughAccountKeys),
        (
            "unsigned payer",
            Box::new(|f| {
                f[3] = Fake::new(
                    Address::new_from_array([1; 32]),
                    pinocchio_system::ID,
                    false,
                    true,
                    &[],
                )
            }),
            MissingRequiredSignature,
        ),
        (
            "read-only payer",
            Box::new(|f| {
                f[3] = Fake::new(
                    Address::new_from_array([1; 32]),
                    pinocchio_system::ID,
                    true,
                    false,
                    &[],
                )
            }),
            Immutable,
        ),
        (
            "another token program",
            Box::new(|f| f[4] = program(Address::new_from_array([9; 32]))),
            IncorrectProgramId,
        ),
        (
            "another system program",
            Box::new(|f| f[5] = program(Address::new_from_array([9; 32]))),
            IncorrectProgramId,
        ),
        (
            "a mint not the token program's",
            Box::new(|f| f[2] = Fake::new(MINT, PROGRAM, false, false, &mint_data())),
            InvalidAccountOwner,
        ),
        (
            "an uninitialized mint",
            Box::new(|f| f[2] = Fake::new(MINT, pinocchio_token::ID, false, false, &[0; 82])),
            UninitializedAccount,
        ),
        (
            "a token account as the mint",
            Box::new(|f| f[2] = token(MINT, &MINT, &MINT)),
            InvalidAccountData,
        ),
        (
            "the pool off its PDA",
            Box::new(|f| f[0] = system(Address::new_from_array([5; 32]))),
            InvalidSeeds,
        ),
        (
            "the vault at the pool's PDA",
            Box::new(|f| f[1] = system(*f[0].view().address())),
            InvalidSeeds,
        ),
        (
            "a read-only pool",
            Box::new(|f| {
                f[0] = Fake::new(
                    *f[0].view().address(),
                    pinocchio_system::ID,
                    false,
                    false,
                    &[],
                )
            }),
            Immutable,
        ),
    ];
    for (what, change, want) in cases {
        assert_eq!(initialize_with(change), Err(want), "{what}");
    }
}

#[test]
fn initialize_refuses_a_second_time() {
    // Created already: the pool is this program's, the vault the token program's.
    let pool = |f: &mut Vec<Fake>| {
        f[0] = Fake::new(
            *f[0].view().address(),
            PROGRAM,
            false,
            true,
            &[0; Pool::LEN],
        )
    };
    assert_eq!(
        initialize_with(pool),
        Err(ProgramError::AccountAlreadyInitialized)
    );
    let vault = |f: &mut Vec<Fake>| f[1] = token(*f[1].view().address(), &MINT, &MINT);
    assert_eq!(
        initialize_with(vault),
        Err(ProgramError::AccountAlreadyInitialized)
    );
    // A system account that holds data is not fresh either.
    let data = |f: &mut Vec<Fake>| {
        f[0] = Fake::new(
            *f[0].view().address(),
            pinocchio_system::ID,
            false,
            true,
            &[0],
        )
    };
    assert_eq!(
        initialize_with(data),
        Err(ProgramError::AccountAlreadyInitialized)
    );
}

// ---- transact ----------------------------------------------------------------------

/// A pool's data, as `initialize` writes it.
fn pool_data() -> Vec<u8> {
    let mut data = vec![0u8; Pool::LEN];
    let state = Pool::load_mut(&mut data).unwrap();
    state.mint = *MINT.as_array();
    state.roots[0] = EMPTY_ROOT;
    state.bump = pda(&[POOL_SEED, MINT.as_ref()]).1;
    state.vault_bump = pda(&[VAULT_SEED, MINT.as_ref()]).1;
    data
}

const NF: [[u8; 32]; 2] = [[11; 32], [12; 32]];

fn args_data(vpub_in: u64, vpub_out: u64, fee: u64) -> Vec<u8> {
    let mut data = vec![0u8; 256];
    data.extend_from_slice(&EMPTY_ROOT);
    data.extend_from_slice(&NF[0]);
    data.extend_from_slice(&NF[1]);
    data.extend_from_slice(&[13; 32]);
    data.extend_from_slice(&[14; 32]);
    data.extend_from_slice(&[15; 32]);
    for x in [vpub_in, vpub_out, fee] {
        data.extend_from_slice(&x.to_le_bytes());
    }
    for memo in [&b"zero"[..], b"one"] {
        data.extend_from_slice(&(memo.len() as u32).to_le_bytes());
        data.extend_from_slice(memo);
    }
    data
}

fn transact_fakes() -> Vec<Fake> {
    let (pool, _) = pda(&[POOL_SEED, MINT.as_ref()]);
    let (vault, _) = pda(&[VAULT_SEED, MINT.as_ref()]);
    let nullifier = |nf: &[u8; 32]| system(pda(&[NULLIFIER_SEED, MINT.as_ref(), nf]).0);
    let user = Address::new_from_array([1; 32]);
    vec![
        Fake::new(pool, PROGRAM, false, true, &pool_data()),
        token(vault, &MINT, &pool),
        nullifier(&NF[0]),
        nullifier(&NF[1]),
        wallet(),
        token(Address::new_from_array([21; 32]), &MINT, &user),
        token(
            Address::new_from_array([22; 32]),
            &MINT,
            &Address::new_from_array([32; 32]),
        ),
        program(PROGRAM), // no relayer
        program(pinocchio_token::ID),
        program(pinocchio_system::ID),
    ]
}

fn transact_with(change: impl FnOnce(&mut Vec<Fake>)) -> Result<(), ProgramError> {
    let mut fakes = transact_fakes();
    change(&mut fakes);
    let data = args_data(5, 5, 0);
    Transact::try_from((&data[..], &mut views(&mut fakes)[..])).map(|_| ())
}

#[test]
fn transact_takes_its_accounts() {
    let mut fakes = transact_fakes();
    let data = args_data(5, 5, 0);
    let mut views = views(&mut fakes);
    let t = Transact::try_from((&data[..], &mut views[..])).unwrap();
    let a = &t.accounts;
    assert!(a.user_token.is_some() && a.recipient_token.is_some());
    assert!(a.relayer_token.is_none(), "the program's address is none");
    for (i, bump) in t.nullifier_bumps.iter().enumerate() {
        assert_eq!(*bump, pda(&[NULLIFIER_SEED, MINT.as_ref(), &NF[i]]).1);
    }
}

#[test]
fn transact_refuses_bad_accounts() {
    use ProgramError::*;
    let other_mint = Address::new_from_array([8; 32]);
    let wrong_mint = ShieldedTransferError::WrongMint.into();
    let addr = |f: &mut Fake| *f.view().address();
    let cases: [Case<'_>; 21] = [
        ("too few", Box::new(|f| drop(f.pop())), NotEnoughAccountKeys),
        (
            "unsigned user",
            Box::new(|f| {
                f[4] = Fake::new(
                    Address::new_from_array([1; 32]),
                    pinocchio_system::ID,
                    false,
                    true,
                    &[],
                )
            }),
            MissingRequiredSignature,
        ),
        (
            "read-only user",
            Box::new(|f| {
                f[4] = Fake::new(
                    Address::new_from_array([1; 32]),
                    pinocchio_system::ID,
                    true,
                    false,
                    &[],
                )
            }),
            Immutable,
        ),
        (
            "another token program",
            Box::new(|f| f[8] = program(Address::new_from_array([9; 32]))),
            IncorrectProgramId,
        ),
        (
            "another system program",
            Box::new(|f| f[9] = program(Address::new_from_array([9; 32]))),
            IncorrectProgramId,
        ),
        (
            "a pool not this program's",
            Box::new(|f| {
                f[0] = Fake::new(
                    addr(&mut f[0]),
                    Address::new_from_array([9; 32]),
                    false,
                    true,
                    &pool_data(),
                )
            }),
            InvalidAccountOwner,
        ),
        (
            "a pool of another size",
            Box::new(|f| {
                f[0] = Fake::new(addr(&mut f[0]), PROGRAM, false, true, &[0; Pool::LEN - 1])
            }),
            InvalidAccountData,
        ),
        (
            "a read-only pool",
            Box::new(|f| f[0] = Fake::new(addr(&mut f[0]), PROGRAM, false, false, &pool_data())),
            Immutable,
        ),
        (
            "a pool off its PDA",
            Box::new(|f| {
                f[0] = Fake::new(
                    Address::new_from_array([5; 32]),
                    PROGRAM,
                    false,
                    true,
                    &pool_data(),
                )
            }),
            InvalidSeeds,
        ),
        (
            "a vault off its PDA",
            Box::new(|f| f[1] = token(Address::new_from_array([5; 32]), &MINT, &addr(&mut f[0]))),
            InvalidSeeds,
        ),
        (
            "a vault not the token program's",
            Box::new(|f| {
                f[1] = Fake::new(
                    addr(&mut f[1]),
                    PROGRAM,
                    false,
                    true,
                    &token_data(&MINT, &MINT, 0),
                )
            }),
            InvalidAccountData,
        ),
        (
            "a vault of another mint",
            Box::new(move |f| f[1] = token(addr(&mut f[1]), &other_mint, &addr(&mut f[0]))),
            wrong_mint,
        ),
        (
            "a vault the pool doesn't own",
            Box::new(|f| f[1] = token(addr(&mut f[1]), &MINT, &MINT)),
            IncorrectAuthority,
        ),
        (
            "a read-only nullifier",
            Box::new(|f| {
                f[2] = Fake::new(addr(&mut f[2]), pinocchio_system::ID, false, false, &[])
            }),
            Immutable,
        ),
        (
            "a nullifier off its PDA",
            Box::new(|f| f[2] = system(Address::new_from_array([5; 32]))),
            InvalidSeeds,
        ),
        (
            "the nullifiers swapped",
            Box::new(|f| f.swap(2, 3)),
            InvalidSeeds,
        ),
        (
            "a spent nullifier",
            Box::new(|f| f[3] = Fake::new(addr(&mut f[3]), PROGRAM, false, true, &[])),
            ShieldedTransferError::NullifierSpent.into(),
        ),
        (
            "a token account of another mint",
            Box::new(move |f| f[5] = token(addr(&mut f[5]), &other_mint, &MINT)),
            ShieldedTransferError::WrongMint.into(),
        ),
        (
            "a read-only token account",
            Box::new(|f| {
                f[6] = Fake::new(
                    addr(&mut f[6]),
                    pinocchio_token::ID,
                    false,
                    false,
                    &token_data(&MINT, &MINT, 0),
                )
            }),
            Immutable,
        ),
        (
            "a token account not the token program's",
            Box::new(|f| {
                f[7] = Fake::new(
                    Address::new_from_array([23; 32]),
                    PROGRAM,
                    false,
                    true,
                    &token_data(&MINT, &MINT, 0),
                )
            }),
            InvalidAccountData,
        ),
        (
            "an uninitialized token account",
            Box::new(|f| {
                f[6] = Fake::new(addr(&mut f[6]), pinocchio_token::ID, false, true, &[0; 165])
            }),
            UninitializedAccount,
        ),
    ];
    for (what, change, want) in cases {
        assert_eq!(transact_with(change), Err(want), "{what}");
    }
}

#[test]
fn transact_parses_exactly_its_arguments() {
    let data = args_data(1, 2, 3);
    let args = TransactInstructionData::try_from(&data[..]).unwrap();
    assert_eq!((args.vpub_in, args.vpub_out, args.fee), (1, 2, 3));
    assert_eq!(args.memos, [&b"zero"[..], b"one"]);
    assert_eq!(args.nf, [&NF[0], &NF[1]]);
    let err = Err(ProgramError::InvalidInstructionData);
    assert_eq!(
        TransactInstructionData::try_from(&data[..data.len() - 1]).map(|_| ()),
        err
    );
    assert_eq!(
        TransactInstructionData::try_from(&[&data[..], &[0]].concat()[..]).map(|_| ()),
        err
    );
    assert_eq!(
        TransactInstructionData::try_from(&data[..400]).map(|_| ()),
        err
    );
}

#[test]
fn the_ring_remembers_sixteen_roots() {
    let mut data = pool_data();
    let pool = Pool::load_mut(&mut data).unwrap();
    assert!(pool.knows_root(&EMPTY_ROOT));
    assert!(!pool.knows_root(&[0; 32]), "all-zero never matches");
    for i in 1..=ROOT_HISTORY_SIZE as u8 {
        pool.push_root(&[i; 32]).unwrap();
        assert_eq!(pool.current_root(), &[i; 32]);
    }
    assert!(!pool.knows_root(&EMPTY_ROOT), "pushed out");
    assert!((1..=ROOT_HISTORY_SIZE as u8).all(|i| pool.knows_root(&[i; 32])));
    assert_eq!(
        u32::from_le_bytes(pool.next_index),
        2 * ROOT_HISTORY_SIZE as u32
    );
    pool.next_index = (u32::MAX - 1).to_le_bytes();
    assert_eq!(
        pool.push_root(&[1; 32]),
        Err(ProgramError::ArithmeticOverflow)
    );
}

// ---- public inputs -----------------------------------------------------------------

/// The demo proof (`examples/demo.rs`: asset `[7; 32]`, recipient `[9; 32]`, relayer
/// `[8; 32]`, a deposit of 150 into the empty pool) verifies against the public
/// inputs the program assembles from its state and the instruction, and not for
/// another recipient, relayer, asset, amount or memo.
#[test]
fn the_demo_proof_verifies_against_what_the_program_assembles() {
    let dir = "../circuits/transact/shielded_transact-verifier";
    let proof: [u8; 256] = std::fs::read(format!("{dir}/proof.solana.bin"))
        .unwrap()
        .try_into()
        .unwrap();
    let want = std::fs::read(format!("{dir}/public_inputs.solana.bin")).unwrap();
    let want: Vec<[u8; 32]> = want.chunks(32).map(|c| c.try_into().unwrap()).collect();
    assert_eq!(want.len(), NUM_PUBLIC_INPUTS);
    let proof = Proof::from_le_bytes(&proof);

    let data = pool_data();
    let pool = Pool::load(&data).unwrap();
    let memos = [[1u8; 32], [2u8; 32]];
    let ix = |vpub_in: u64| {
        let mut ix = proof_args(&want);
        ix.extend_from_slice(&vpub_in.to_le_bytes());
        ix.extend_from_slice(&[0; 16]);
        ix
    };
    // The demo binds its memos' hashes directly; the program hashes what it's sent,
    // so assemble with the hashes in place to compare.
    let assemble = |vpub_in: u64, recipient: &[u8; 32], relayer: &[u8; 32]| {
        let mut ix = ix(vpub_in);
        for _ in &memos {
            ix.extend_from_slice(&0u32.to_le_bytes());
        }
        let args = TransactInstructionData::try_from(&ix[..]).unwrap();
        let mut inputs = public_inputs(pool, &args, recipient, relayer);
        for (i, memo) in memos.iter().enumerate() {
            [inputs[15 + 2 * i], inputs[16 + 2 * i]] = halves(memo);
        }
        inputs
    };
    let inputs = assemble(150, &[9; 32], &[8; 32]);
    assert_eq!(inputs.as_slice(), want.as_slice());
    assert!(verify(&proof, &inputs));
    assert!(!verify(&proof, &assemble(150, &[8; 32], &[8; 32])));
    assert!(!verify(&proof, &assemble(150, &[9; 32], &[9; 32])));
    assert!(!verify(&proof, &assemble(151, &[9; 32], &[8; 32])));
    let mut other = inputs;
    other[1] = halves(&[6; 32])[0];
    assert!(!verify(&proof, &other));
    let mut other = inputs;
    other[15] = halves(&[3; 32])[0];
    assert!(!verify(&proof, &other));
}

/// The memo halves are the SHA-256 of the memo bytes sent.
#[test]
fn memos_are_bound_by_their_sha256() {
    let data = args_data(0, 0, 0);
    let args = TransactInstructionData::try_from(&data[..]).unwrap();
    let mut pool_bytes = pool_data();
    let pool = Pool::load_mut(&mut pool_bytes).unwrap();
    let inputs = public_inputs(pool, &args, &[0; 32], &[0; 32]);
    let digest: [u8; 32] = hash(b"zero").to_bytes();
    assert_eq!([inputs[15], inputs[16]], halves(&digest));
    assert_eq!(inputs[10], [0; 32]);
}

/// The instruction data before the amounts, from the demo's public inputs: a zero
/// proof (unused), `root`, the nullifiers, the commitments, `new_root`.
fn proof_args(inputs: &[[u8; 32]]) -> Vec<u8> {
    let mut ix = vec![0u8; 256];
    for i in [0, 3, 4, 5, 6, 8] {
        ix.extend_from_slice(&inputs[i]);
    }
    ix
}
