//! Shielded pool: the onchain half (Pinocchio).
//!
//! The program never hashes a tree node. All Poseidon lives in the circuits; the
//! chain keeps only what consensus needs (a ring of recent Merkle roots, the next
//! leaf index, and one marker account per spent nullifier) and verifies two proofs
//! with the crates `xark export` generated:
//!
//!   deposit   "new_root extends the current tree by one leaf"  → store new_root
//!   withdraw  "I own some leaf, here is its nullifier"          → pay out once
//!
//! Each proof is checked against public inputs the program assembles itself,
//! 32-byte little-endian field elements in the circuit's order, so a proof only
//! holds for the state and accounts of the transaction carrying it.
//!
//! Instruction data is a one-byte tag, then the arguments' bytes (see
//! [`INITIALIZE`], [`DEPOSIT`], [`WITHDRAW`]). Nothing checks an account for the
//! program: each check is written out, in [`accounts`] and in each instruction.
//!
//! Reference implementation: unaudited, educational. See the README.
#![cfg_attr(not(test), no_std)]

use pinocchio::{
    AccountView, Address, ProgramResult,
    cpi::{Seed, Signer},
    error::ProgramError,
    program_entrypoint,
};
use shielded_pool_deposit_xark_verifier as deposit_verifier;
use shielded_pool_deposit_xark_verifier::Proof; // the same type in both verifier crates
use shielded_pool_withdraw_xark_verifier as withdraw_verifier;

program_entrypoint!(process_instruction);
#[cfg(not(test))]
pinocchio::nostd_panic_handler!();
#[cfg(not(test))]
pinocchio::no_allocator!();

const TREE_HEIGHT: u32 = 20;
const ROOT_HISTORY_SIZE: usize = 16;

const POOL_SEED: &[u8] = b"pool";
const NULLIFIER_SEED: &[u8] = b"nullifier";

// The empty tree's root, little-endian: the first deposit's `old_root`. Fixed
// here so a pool can't be seeded with another; the client tests check it against
// the circuit's `Path.first().treeRoot()`, and the host tests against the deposit
// demo's `old_root`.
const EMPTY_ROOT: [u8; 32] = [
    26, 122, 47, 146, 164, 115, 191, 166, 88, 234, 28, 252, 38, 169, 221, 171, 246, 159, 204, 103,
    2, 229, 199, 103, 212, 104, 254, 41, 151, 59, 127, 46,
];

// ---- instructions --------------------------------------------------------------

/// `initialize(denomination: u64)`: data `[0] ‖ denomination (u64 LE)`.
/// Accounts: `pool` (writable, the PDA), `authority` (writable signer, pays),
/// the system program.
pub const INITIALIZE: u8 = 0;
/// `deposit(commitment, new_root, proof)`: data `[1] ‖ commitment (32) ‖
/// new_root (32) ‖ proof (256)`. Accounts: `pool` (writable), `depositor`
/// (writable signer), the system program.
pub const DEPOSIT: u8 = 1;
/// `withdraw(proof, root, nullifier_hash, fee)`: data `[2] ‖ proof (256) ‖ root
/// (32) ‖ nullifier_hash (32) ‖ fee (u64 LE)`. Accounts: `pool` (writable),
/// `nullifier` (writable, the PDA), `recipient` (writable), `relayer` (writable
/// signer, pays), the system program.
pub const WITHDRAW: u8 = 2;

fn process_instruction(
    program_id: &Address,
    accounts: &mut [AccountView],
    data: &[u8],
) -> ProgramResult {
    let result = match data.split_first() {
        Some((&INITIALIZE, args)) => initialize(program_id, accounts, cast(args)?),
        Some((&DEPOSIT, args)) => deposit(program_id, accounts, cast(args)?),
        Some((&WITHDRAW, args)) => withdraw(program_id, accounts, cast(args)?),
        _ => Err(ProgramError::InvalidInstructionData),
    };
    if let Err(ProgramError::Custom(code)) = result {
        log(PoolError::message(code));
    }
    result
}

#[repr(C)]
struct InitializeArgs {
    denomination: [u8; 8],
}

#[repr(C)]
struct DepositArgs {
    commitment: [u8; 32],
    new_root: [u8; 32],
    proof: [u8; 256],
}

#[repr(C)]
struct WithdrawArgs {
    proof: [u8; 256],
    root: [u8; 32],
    nullifier_hash: [u8; 32],
    fee: [u8; 8],
}

/// Create the pool for one denomination. Permissionless: the first caller fixes
/// the size (fine for a demo); the empty-tree root is a program constant, so the
/// pool cannot be seeded with a wrong root.
fn initialize(
    program_id: &Address,
    accounts: &mut [AccountView],
    args: &InitializeArgs,
) -> ProgramResult {
    let [pool, authority, system, ..] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    accounts::writable_signer(authority)?;
    accounts::system_program(system)?;
    accounts::writable(pool)?;
    // One pool per denomination (like Tornado's separate per-size pools), at the
    // canonical bump.
    let (address, bump) =
        Address::find_program_address(&[POOL_SEED, &args.denomination], program_id);
    if pool.address() != &address {
        return Err(ProgramError::InvalidSeeds);
    }
    if !accounts::is_uninitialized(pool) {
        return Err(ProgramError::AccountAlreadyInitialized);
    }

    let bump = [bump];
    let seeds = [
        Seed::from(POOL_SEED),
        Seed::from(&args.denomination),
        Seed::from(&bump),
    ];
    pinocchio_system::create_account_with_minimum_balance_signed(
        pool,
        Pool::LEN,
        program_id,
        authority,
        None,
        &[Signer::from(&seeds)],
    )?;

    let mut data = pool.try_borrow_mut()?;
    let state = Pool::from_bytes_mut(&mut data)?; // created zeroed: the rest of the ring is empty
    state.tag = Pool::TAG;
    state.bump = bump[0];
    state.denomination = args.denomination;
    state.roots[0] = EMPTY_ROOT;
    Ok(())
}

/// Deposit the fixed denomination and append `commitment` to the tree. The proof
/// attests that `new_root` correctly extends the current tree by one leaf, so the
/// program can advance the tree without hashing.
fn deposit(
    program_id: &Address,
    accounts: &mut [AccountView],
    args: &DepositArgs,
) -> ProgramResult {
    let [pool, depositor, system, ..] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    accounts::pool(program_id, pool)?;
    accounts::writable_signer(depositor)?;
    accounts::system_program(system)?;

    let (denomination, index) = {
        let mut data = pool.try_borrow_mut()?;
        let state = Pool::from_bytes_mut(&mut data)?;
        let index = state.next_index();
        if u64::from(index) >= 1u64 << TREE_HEIGHT {
            return Err(PoolError::TreeFull.into());
        }

        // The deposit circuit's public `Append`, field by field.
        let public_inputs = deposit_inputs(
            state.current_root(),
            &args.new_root,
            &args.commitment,
            index,
        );
        if !deposit_verifier::verify(&Proof::from_le_bytes(&args.proof), &public_inputs) {
            return Err(PoolError::InvalidProof.into());
        }

        // Advance the tree: remember the new root, bump the index.
        state.push_root(&args.new_root);
        (state.denomination(), index)
    };

    // Pull the fixed denomination from the depositor into the pool PDA.
    pinocchio_system::instructions::Transfer {
        from: depositor,
        to: pool,
        lamports: denomination,
    }
    .invoke()?;

    // The event a wallet indexes the tree from: the commitment and its leaf index.
    log_data(&[&args.commitment, &index.to_le_bytes()]);
    Ok(())
}

/// Withdraw the denomination (minus `fee`) to `recipient`, paying `fee` to the
/// relayer. The proof binds recipient, relayer and fee, so nobody can re-target
/// it. The `nullifier` account is created here, and its existence is the "already
/// spent" check: no double-spend.
///
/// Relayer economics: the relayer pays the (non-refundable) nullifier rent and the
/// transaction fee, and is compensated only by `fee`. `fee` is chosen by the
/// depositor and bound into the proof, so there's no onchain floor: a relayer must
/// check `fee` covers its costs offchain before submitting.
fn withdraw(
    program_id: &Address,
    accounts: &mut [AccountView],
    args: &WithdrawArgs,
) -> ProgramResult {
    let [pool, nullifier, recipient, relayer, system, ..] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    accounts::pool(program_id, pool)?;
    accounts::writable_signer(relayer)?;
    accounts::system_program(system)?;
    accounts::writable(recipient)?;
    // Only receives lamports, and its identity is bound into the proof; but never
    // one of the pool's own accounts, whose lamports the payout below moves.
    if recipient.address() == pool.address() || recipient.address() == nullifier.address() {
        return Err(ProgramError::InvalidArgument);
    }

    let denomination = {
        let data = pool.try_borrow()?;
        let state = Pool::from_bytes(&data)?;
        let denomination = state.denomination();

        // The nullifier's marker account, at its canonical bump, namespaced by the
        // denomination. It exists once the note is spent.
        accounts::writable(nullifier)?;
        let seeds = [NULLIFIER_SEED, &state.denomination, &args.nullifier_hash];
        let (address, bump) = Address::find_program_address(&seeds, program_id);
        if nullifier.address() != &address {
            return Err(ProgramError::InvalidSeeds);
        }
        if !accounts::is_uninitialized(nullifier) {
            return Err(PoolError::NullifierSpent.into());
        }

        let fee = u64::from_le_bytes(args.fee);
        if fee > denomination {
            return Err(PoolError::FeeTooHigh.into());
        }
        if !state.knows_root(&args.root) {
            return Err(PoolError::UnknownRoot.into());
        }

        // The withdraw circuit's public `Withdrawal`, field by field.
        let public_inputs = withdraw_inputs(
            &args.root,
            &args.nullifier_hash,
            recipient.address(),
            relayer.address(),
            fee,
        );
        if !withdraw_verifier::verify(&Proof::from_le_bytes(&args.proof), &public_inputs) {
            return Err(PoolError::InvalidProof.into());
        }

        // Spend the note: create its marker, paid by the relayer.
        let bump = [bump];
        let seeds = [
            Seed::from(NULLIFIER_SEED),
            Seed::from(&state.denomination),
            Seed::from(&args.nullifier_hash),
            Seed::from(&bump),
        ];
        pinocchio_system::create_account_with_minimum_balance_signed(
            nullifier,
            Nullifier::LEN,
            program_id,
            relayer,
            None,
            &[Signer::from(&seeds)],
        )?;
        denomination
    };
    nullifier.try_borrow_mut()?[0] = Nullifier::TAG;

    // Pay out from the pool PDA (program-owned, so its lamports move directly).
    // `fee <= denomination` above; the runtime keeps the pool rent-exempt. Each
    // move reads the balance it changes, so a recipient that is the relayer gets
    // both.
    let fee = u64::from_le_bytes(args.fee);
    debit(pool, denomination)?;
    credit(recipient, denomination - fee)?;
    credit(relayer, fee)
}

fn debit(account: &mut AccountView, lamports: u64) -> ProgramResult {
    let balance = account
        .lamports()
        .checked_sub(lamports)
        .ok_or(ProgramError::InsufficientFunds)?;
    account.set_lamports(balance);
    Ok(())
}

fn credit(account: &mut AccountView, lamports: u64) -> ProgramResult {
    let balance = account
        .lamports()
        .checked_add(lamports)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    account.set_lamports(balance);
    Ok(())
}

// ---- accounts ------------------------------------------------------------------

/// What each kind of account must be, one function each.
mod accounts {
    use super::*;

    /// An account the program writes to.
    pub fn writable(account: &AccountView) -> ProgramResult {
        if !account.is_writable() {
            return Err(ProgramError::Immutable);
        }
        Ok(())
    }

    /// A payer: a signer, writable.
    pub fn writable_signer(account: &AccountView) -> ProgramResult {
        if !account.is_signer() {
            return Err(ProgramError::MissingRequiredSignature);
        }
        writable(account)
    }

    /// The system program, which creates the accounts and moves the deposits.
    pub fn system_program(account: &AccountView) -> ProgramResult {
        if account.address() != &pinocchio_system::ID {
            return Err(ProgramError::IncorrectProgramId);
        }
        Ok(())
    }

    /// An account about to be created: nothing lives at the address yet. A system-owned
    /// account with no data may hold lamports (anyone can send them), which
    /// creation tops up to rent exemption rather than failing on.
    pub fn is_uninitialized(account: &AccountView) -> bool {
        account.owned_by(&pinocchio_system::ID) && account.is_data_empty()
    }

    /// A pool: writable, owned by this program, a pool's size and tag, at the
    /// address its own denomination and stored bump derive (`["pool",
    /// denomination]`). The bump is the canonical one `initialize` found.
    pub fn pool(program_id: &Address, pool: &AccountView) -> ProgramResult {
        writable(pool)?;
        if !pool.owned_by(program_id) {
            return Err(ProgramError::InvalidAccountOwner);
        }
        let data = pool.try_borrow()?;
        let state = Pool::from_bytes(&data)?;
        let address = Address::derive_address(
            &[POOL_SEED, &state.denomination],
            Some(state.bump),
            program_id,
        );
        if pool.address() != &address {
            return Err(ProgramError::InvalidSeeds);
        }
        Ok(())
    }
}

// ---- state ---------------------------------------------------------------------

/// Plain bytes: `#[repr(C)]`, alignment 1, every bit pattern valid, so a byte
/// slice of exactly its size can be read as one.
///
/// # Safety
///
/// The implementor must be `#[repr(C)]` and made only of `u8` arrays.
unsafe trait Plain: Sized {
    const LEN: usize = size_of::<Self>();
}

unsafe impl Plain for InitializeArgs {}
unsafe impl Plain for DepositArgs {}
unsafe impl Plain for WithdrawArgs {}
unsafe impl Plain for Pool {}

/// The arguments `data` holds, if it is exactly their size.
fn cast<T: Plain>(data: &[u8]) -> Result<&T, ProgramError> {
    if data.len() != T::LEN {
        return Err(ProgramError::InvalidInstructionData);
    }
    // SAFETY: `T` is plain bytes (alignment 1) and `data` is exactly its size.
    Ok(unsafe { &*data.as_ptr().cast::<T>() })
}

/// A pool's account: 536 bytes.
///
/// | offset | size | field |
/// |---|---|---|
/// | 0 | 1 | tag, [`Pool::TAG`] |
/// | 1 | 1 | bump |
/// | 2 | 6 | reserved, zero |
/// | 8 | 8 | denomination (u64 LE) |
/// | 16 | 4 | next_index (u32 LE) |
/// | 20 | 4 | current_root_index (u32 LE) |
/// | 24 | 512 | roots: 16 × 32-byte little-endian field elements, a ring |
#[repr(C)]
struct Pool {
    tag: u8,
    bump: u8,
    reserved: [u8; 6],
    denomination: [u8; 8],
    next_index: [u8; 4],
    current_root_index: [u8; 4],
    roots: [[u8; 32]; ROOT_HISTORY_SIZE],
}

const _: () = assert!(Pool::LEN == 536 && align_of::<Pool>() == 1);

impl Pool {
    const TAG: u8 = 1;

    /// The pool `data` holds, if it is a pool's size and tag.
    fn from_bytes(data: &[u8]) -> Result<&Pool, ProgramError> {
        if data.len() != Pool::LEN || data[0] != Pool::TAG {
            return Err(ProgramError::InvalidAccountData);
        }
        // SAFETY: `Pool` is plain bytes and `data` is exactly its size.
        Ok(unsafe { &*data.as_ptr().cast::<Pool>() })
    }

    /// As [`Pool::from_bytes`], to write; a freshly created account (tag 0) too.
    fn from_bytes_mut(data: &mut [u8]) -> Result<&mut Pool, ProgramError> {
        if data.len() != Pool::LEN || (data[0] != Pool::TAG && data[0] != 0) {
            return Err(ProgramError::InvalidAccountData);
        }
        // SAFETY: as above.
        Ok(unsafe { &mut *data.as_mut_ptr().cast::<Pool>() })
    }

    fn denomination(&self) -> u64 {
        u64::from_le_bytes(self.denomination)
    }

    fn next_index(&self) -> u32 {
        u32::from_le_bytes(self.next_index)
    }

    fn current_root_index(&self) -> usize {
        u32::from_le_bytes(self.current_root_index) as usize % ROOT_HISTORY_SIZE
    }

    fn current_root(&self) -> &[u8; 32] {
        &self.roots[self.current_root_index()]
    }

    /// Remember `root` as the newest entry in the ring and advance the tree. The
    /// caller has checked the tree isn't full, so the index doesn't overflow.
    fn push_root(&mut self, root: &[u8; 32]) {
        let next = (self.current_root_index() + 1) % ROOT_HISTORY_SIZE;
        self.current_root_index = (next as u32).to_le_bytes();
        self.roots[next] = *root;
        self.next_index = (self.next_index() + 1).to_le_bytes();
    }

    /// Is `root` one of the recent ring entries? (All-zero never matches: the
    /// unused ring slots are zeroed, and a real root is never zero.)
    fn knows_root(&self, root: &[u8; 32]) -> bool {
        root.iter().any(|&b| b != 0) && self.roots.iter().any(|r| r == root)
    }
}

/// A spent nullifier's marker account: one byte, its tag. Its existence is what
/// counts.
struct Nullifier;

impl Nullifier {
    const LEN: usize = 1;
    const TAG: u8 = 2;
}

// ---- errors --------------------------------------------------------------------

/// The program's own errors, as `ProgramError::Custom` codes from 6000. The entrypoint logs each one's name and message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum PoolError {
    /// invalid proof
    InvalidProof = 6000,
    /// unknown or stale merkle root
    UnknownRoot,
    /// fee exceeds denomination
    FeeTooHigh,
    /// tree is full
    TreeFull,
    /// nullifier already spent
    NullifierSpent,
}

impl From<PoolError> for ProgramError {
    fn from(error: PoolError) -> Self {
        ProgramError::Custom(error as u32)
    }
}

impl PoolError {
    /// What the program logs for error `code`: the name, which the client tests
    /// match, and the message.
    fn message(code: u32) -> &'static str {
        const MESSAGES: [&str; 5] = [
            "InvalidProof: invalid proof",
            "UnknownRoot: unknown or stale merkle root",
            "FeeTooHigh: fee exceeds denomination",
            "TreeFull: tree is full",
            "NullifierSpent: nullifier already spent",
        ];
        let index = code.wrapping_sub(PoolError::InvalidProof as u32) as usize;
        MESSAGES.get(index).copied().unwrap_or("unknown error")
    }
}

fn log(message: &str) {
    #[cfg(any(target_os = "solana", target_arch = "bpf"))]
    // SAFETY: the pointer and length are a valid `&str`'s.
    unsafe {
        pinocchio::syscalls::sol_log_(message.as_ptr(), message.len() as u64)
    };
    #[cfg(not(any(target_os = "solana", target_arch = "bpf")))]
    let _ = message;
}

/// `Program data: <base64> <base64> ..`, one per slice.
fn log_data(data: &[&[u8]]) {
    #[cfg(any(target_os = "solana", target_arch = "bpf"))]
    // SAFETY: `data` is a valid slice of slices, the layout the syscall reads.
    unsafe {
        pinocchio::syscalls::sol_log_data(data.as_ptr().cast(), data.len() as u64)
    };
    #[cfg(not(any(target_os = "solana", target_arch = "bpf")))]
    let _ = data;
}

// ---- public inputs -------------------------------------------------------------

/// The deposit circuit's `Append`: `old_root, new_root, leaf, index`.
fn deposit_inputs(
    old_root: &[u8; 32],
    new_root: &[u8; 32],
    leaf: &[u8; 32],
    index: u32,
) -> [[u8; 32]; 4] {
    [
        *old_root,
        *new_root,
        *leaf,
        field_from_u64(u64::from(index)),
    ]
}

/// The withdraw circuit's `Withdrawal`: `root, nullifier_hash, recipient (hi,
/// lo), relayer (hi, lo), fee`. An address is wider than the field, so it enters
/// as its two halves (`Address`).
fn withdraw_inputs(
    root: &[u8; 32],
    nullifier_hash: &[u8; 32],
    recipient: &Address,
    relayer: &Address,
    fee: u64,
) -> [[u8; 32]; 7] {
    let [r_hi, r_lo] = address_halves(recipient.as_array());
    let [l_hi, l_lo] = address_halves(relayer.as_array());
    [
        *root,
        *nullifier_hash,
        r_hi,
        r_lo,
        l_hi,
        l_lo,
        field_from_u64(fee),
    ]
}

/// A u64 as a 32-byte little-endian field element.
fn field_from_u64(x: u64) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[..8].copy_from_slice(&x.to_le_bytes());
    out
}

/// An address as the circuit's `Address`: `[hi, lo]`, each 16-byte half read as a
/// big-endian integer, as 32-byte little-endian field elements.
fn address_halves(bytes: &[u8; 32]) -> [[u8; 32]; 2] {
    let half = |b: &[u8]| {
        let mut out = [0u8; 32];
        out[..16].copy_from_slice(b);
        out[..16].reverse();
        out
    };
    [half(&bytes[..16]), half(&bytes[16..])]
}

#[cfg(test)]
mod tests;
