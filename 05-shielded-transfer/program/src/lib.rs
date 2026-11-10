//! Shielded transfer, the onchain half (Pinocchio + SPL).
//!
//! One `transact` instruction covers deposit, private transfer and withdraw through
//! the JoinSplit value slots. The chain never hashes a note: it verifies the proof,
//! checks the anchor root is remembered, burns the two nullifiers, advances the tree
//! to `new_root`, moves SPL for the public value slots, and logs the two proof-bound
//! encrypted memos for scanning wallets.
//!
//! The proof is checked against the circuit's 19 public inputs, which the program
//! assembles itself from its state and the transaction: 32-byte little-endian field
//! elements, in the circuit's order.
//!
//! Instructions are a one-byte tag, then their arguments (see [`Instruction`]).
//! With no framework to check accounts, every check is written out, in
//! [`initialize_accounts`] and [`transact_accounts`], which run before any state
//! changes.
//!
//! Reference implementation, unaudited and educational. See the README.
#![cfg_attr(not(test), no_std)]

use pinocchio::{
    AccountView, Address, ProgramResult,
    cpi::{Seed, Signer},
    error::ProgramError,
    program_entrypoint,
};
use pinocchio_system::create_account_with_minimum_balance_signed;
use pinocchio_token::{
    instructions::{InitializeAccount3, Transfer},
    state::{Account as TokenAccount, Mint},
};
use shielded_transfer_transact_xark_verifier::{NUM_PUBLIC_INPUTS, Proof, verify};
use solana_sha256_hasher::hash;

program_entrypoint!(process_instruction);
#[cfg(not(test))]
pinocchio::nostd_panic_handler!();
#[cfg(not(test))]
pinocchio::no_allocator!();

const TREE_HEIGHT: u32 = 20;
const ROOT_HISTORY_SIZE: usize = 16;

const POOL_SEED: &[u8] = b"pool";
const VAULT_SEED: &[u8] = b"vault";
const NULLIFIER_SEED: &[u8] = b"nullifier";

// The empty tree's root, little-endian: the first transaction's `old_root`. The
// client tests check it against the circuit's `Path.first().treeRoot()`.
const EMPTY_ROOT: [u8; 32] = [
    26, 122, 47, 146, 164, 115, 191, 166, 88, 234, 28, 252, 38, 169, 221, 171, 246, 159, 204, 103,
    2, 229, 199, 103, 212, 104, 254, 41, 151, 59, 127, 46,
];

/// A spent nullifier is a program-owned account at its PDA holding no data: its
/// existence is the mark, and the rent-exempt minimum for zero bytes is its cost.
const NULLIFIER_LEN: usize = 0;

// ---- instructions ------------------------------------------------------------------

/// The instruction tags, the first byte of the instruction data.
///
/// - `0` **initialize**, no arguments. Creates the pool and its token vault for an
///   SPL mint (one pool per mint). Accounts: `pool` (writable, PDA
///   `["pool", mint]`), `vault` (writable, PDA `["vault", mint]`), `mint`,
///   `authority` (writable signer, pays the rent), the token program, the system
///   program.
/// - `1` **transact**: `proof` (256 B) ‖ `root` ‖ `nf[0]` ‖ `nf[1]` ‖ `cm_out[0]` ‖
///   `cm_out[1]` ‖ `new_root` (32 B each) ‖ `vpub_in` ‖ `vpub_out` ‖ `fee` (u64 LE
///   each) ‖ `memo0` ‖ `memo1` (each a u32 LE length, then its bytes). Accounts:
///   `pool` (writable), `vault` (writable), `nullifier0` and `nullifier1` (writable,
///   PDAs `["nullifier", mint, nf[i]]`), `user` (writable signer, pays the
///   nullifiers' rent), `user_token`, `recipient_token`, `relayer_token` (each
///   writable, or this program's address for none), the token program, the system
///   program.
#[repr(u8)]
pub enum Instruction {
    Initialize = 0,
    Transact = 1,
}

fn process_instruction(
    program_id: &Address,
    accounts: &mut [AccountView],
    instruction_data: &[u8],
) -> ProgramResult {
    match instruction_data.split_first() {
        Some((&tag, [])) if tag == Instruction::Initialize as u8 => {
            initialize(program_id, accounts)
        }
        Some((&tag, args)) if tag == Instruction::Transact as u8 => {
            transact(program_id, accounts, &TransactArgs::parse(args)?)
        }
        _ => Err(ProgramError::InvalidInstructionData),
    }
}

// ---- initialize --------------------------------------------------------------------

/// Create the pool and its token vault for an SPL mint (one pool per mint).
#[inline(never)]
fn initialize(program_id: &Address, accounts: &[AccountView]) -> ProgramResult {
    let a = initialize_accounts(program_id, accounts)?;
    let mint = a.mint.address().as_array();

    let pool_bump = [a.pool_bump];
    let seeds = [
        Seed::from(POOL_SEED),
        Seed::from(mint),
        Seed::from(&pool_bump),
    ];
    let mut pool = a.pool;
    create_account_with_minimum_balance_signed(
        &mut pool,
        Pool::LEN,
        program_id,
        &a.authority,
        None,
        &[Signer::from(&seeds)],
    )?;
    {
        let mut data = pool.try_borrow_mut()?;
        let state = Pool::load_mut(&mut data)?;
        state.mint = *mint;
        state.next_index = 0u32.to_le_bytes();
        state.current_root_index = 0u32.to_le_bytes();
        state.roots = [[0; 32]; ROOT_HISTORY_SIZE];
        state.roots[0] = EMPTY_ROOT;
        state.bump = a.pool_bump;
        state.vault_bump = a.vault_bump;
    }

    let vault_bump = [a.vault_bump];
    let seeds = [
        Seed::from(VAULT_SEED),
        Seed::from(mint),
        Seed::from(&vault_bump),
    ];
    let mut vault = a.vault;
    create_account_with_minimum_balance_signed(
        &mut vault,
        TokenAccount::LEN,
        &pinocchio_token::ID,
        &a.authority,
        None,
        &[Signer::from(&seeds)],
    )?;
    InitializeAccount3::new(&vault, &a.mint, pool.address()).invoke()
}

/// `initialize`'s accounts, checked.
struct InitializeAccounts {
    pool: AccountView,
    vault: AccountView,
    mint: AccountView,
    authority: AccountView,
    pool_bump: u8,
    vault_bump: u8,
}

/// `initialize`'s account checks: the payer signs and is writable, the two programs
/// are the system and token programs, the mint is an initialized SPL mint, and the
/// pool and vault are writable, at their canonical PDAs, and not yet created
/// (refusing a second initialisation).
#[inline(never)]
fn initialize_accounts(
    program_id: &Address,
    accounts: &[AccountView],
) -> Result<InitializeAccounts, ProgramError> {
    let [
        pool,
        vault,
        mint,
        authority,
        token_program,
        system_program,
        ..,
    ] = accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer(authority)?;
    programs(token_program, system_program)?;
    if !Mint::from_account_view(mint)?.is_initialized() {
        return Err(ProgramError::UninitializedAccount);
    }
    let mint_key = mint.address().as_ref();
    let pool_bump = fresh_pda(pool, &[POOL_SEED, mint_key], program_id)?;
    let vault_bump = fresh_pda(vault, &[VAULT_SEED, mint_key], program_id)?;
    Ok(InitializeAccounts {
        pool: *pool,
        vault: *vault,
        mint: *mint,
        authority: *authority,
        pool_bump,
        vault_bump,
    })
}

// ---- transact ----------------------------------------------------------------------

/// `transact`'s arguments, as the instruction data lays them out.
struct TransactArgs<'a> {
    proof: &'a [u8; 256],
    root: &'a [u8; 32],
    nf: [&'a [u8; 32]; 2],
    cm_out: [&'a [u8; 32]; 2],
    new_root: &'a [u8; 32],
    vpub_in: u64,
    vpub_out: u64,
    fee: u64,
    memos: [&'a [u8]; 2],
}

impl<'a> TransactArgs<'a> {
    /// Read the arguments, refusing short or trailing data.
    fn parse(data: &'a [u8]) -> Result<Self, ProgramError> {
        let mut rest = data;
        let proof = take::<256>(&mut rest)?;
        let root = take::<32>(&mut rest)?;
        let nf = [take::<32>(&mut rest)?, take::<32>(&mut rest)?];
        let cm_out = [take::<32>(&mut rest)?, take::<32>(&mut rest)?];
        let new_root = take::<32>(&mut rest)?;
        let vpub_in = u64::from_le_bytes(*take::<8>(&mut rest)?);
        let vpub_out = u64::from_le_bytes(*take::<8>(&mut rest)?);
        let fee = u64::from_le_bytes(*take::<8>(&mut rest)?);
        let memos = [memo(&mut rest)?, memo(&mut rest)?];
        if !rest.is_empty() {
            return Err(ProgramError::InvalidInstructionData);
        }
        Ok(Self {
            proof,
            root,
            nf,
            cm_out,
            new_root,
            vpub_in,
            vpub_out,
            fee,
            memos,
        })
    }
}

/// The next `N` bytes of `rest`.
fn take<'a, const N: usize>(rest: &mut &'a [u8]) -> Result<&'a [u8; N], ProgramError> {
    let (head, tail) = rest
        .split_first_chunk::<N>()
        .ok_or(ProgramError::InvalidInstructionData)?;
    *rest = tail;
    Ok(head)
}

/// A memo: its length as a u32, then its bytes (Borsh's `Vec<u8>`, as before).
fn memo<'a>(rest: &mut &'a [u8]) -> Result<&'a [u8], ProgramError> {
    let len = u32::from_le_bytes(*take::<4>(rest)?) as usize;
    if rest.len() < len {
        return Err(ProgramError::InvalidInstructionData);
    }
    let (memo, tail) = rest.split_at(len);
    *rest = tail;
    Ok(memo)
}

/// One JoinSplit: spend 2 input notes, create 2 output notes, with public value
/// entering (`vpub_in`) and/or leaving (`vpub_out`, and the relayer's `fee`) the
/// shield.
#[inline(never)]
fn transact(program_id: &Address, accounts: &[AccountView], args: &TransactArgs) -> ProgramResult {
    let a = transact_accounts(program_id, accounts, args)?;
    let mint = a.mint;

    let next_index = {
        let data = a.pool.try_borrow()?;
        let pool = Pool::load(&data)?;
        let next_index = u32::from_le_bytes(pool.next_index);
        if u64::from(next_index) + 2 > 1u64 << TREE_HEIGHT {
            return Err(PoolError::TreeFull.into());
        }
        if !pool.knows_root(args.root) {
            return Err(PoolError::UnknownRoot.into());
        }
        // Without a withdrawal there's no recipient, and without a fee no relayer:
        // the proof binds this program's address in their place.
        let address =
            |a: Option<AccountView>| a.map_or(*program_id.as_array(), |a| *a.address().as_array());
        let (recipient, relayer) = (address(a.recipient_token), address(a.relayer_token));
        if !proves(pool, args, &recipient, &relayer) {
            return Err(PoolError::InvalidProof.into());
        }
        next_index
    };

    // Burn the nullifiers: one account each, created at its PDA by this program.
    for (i, (mut nullifier, bump)) in a.nullifiers.into_iter().enumerate() {
        let bump = [bump];
        let seeds = [
            Seed::from(NULLIFIER_SEED),
            Seed::from(&mint),
            Seed::from(args.nf[i]),
            Seed::from(&bump),
        ];
        create_account_with_minimum_balance_signed(
            &mut nullifier,
            NULLIFIER_LEN,
            program_id,
            &a.user,
            None,
            &[Signer::from(&seeds)],
        )?;
    }

    // Public value entering the shield: user → vault, signed by the user.
    if args.vpub_in > 0 {
        let user_token = a.user_token.ok_or(PoolError::NoSource)?;
        Transfer::new(&user_token, &a.vault, &a.user, args.vpub_in).invoke()?;
    }

    // Public value leaving the shield, signed by the pool PDA: `vpub_out` to the
    // recipient, `fee` to the relayer. Their sum must fit the vault's u64 balance.
    args.vpub_out
        .checked_add(args.fee)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    let bump = [a.pool_bump];
    let seeds = [Seed::from(POOL_SEED), Seed::from(&mint), Seed::from(&bump)];
    let payouts = [
        (args.vpub_out, a.recipient_token, PoolError::NoRecipient),
        (args.fee, a.relayer_token, PoolError::NoRelayer),
    ];
    for (amount, to, missing) in payouts {
        if amount > 0 {
            let to = to.ok_or(missing)?;
            Transfer::new(&a.vault, &to, &a.pool, amount).invoke_signed(&[Signer::from(&seeds)])?;
        }
    }

    // Advance the tree past the two appended leaves and publish the memos (delivery
    // is off-circuit; wallets trial-decrypt to find their notes).
    {
        let mut pool = a.pool;
        let mut data = pool.try_borrow_mut()?;
        Pool::load_mut(&mut data)?.push_root(args.new_root)?;
    }
    for (i, memo) in args.memos.into_iter().enumerate() {
        let leaf_index = next_index + i as u32;
        memo_event(leaf_index, args.cm_out[i], memo);
    }
    Ok(())
}

/// `transact`'s accounts, checked.
struct TransactAccounts {
    pool: AccountView,
    vault: AccountView,
    /// Each with its canonical bump.
    nullifiers: [(AccountView, u8); 2],
    user: AccountView,
    user_token: Option<AccountView>,
    recipient_token: Option<AccountView>,
    relayer_token: Option<AccountView>,
    mint: [u8; 32],
    pool_bump: u8,
}

/// `transact`'s account checks: the user signs and is writable; the two programs
/// are the token and system programs; the pool is this program's, of a pool's size,
/// writable, at its PDA with its stored bump; the vault is writable, at its PDA, and
/// the pool's token account of the pool's mint; each nullifier is writable, at its
/// canonical PDA, and not yet spent; each token account given is writable, an
/// initialized SPL token account, and of the pool's mint.
#[inline(never)]
fn transact_accounts(
    program_id: &Address,
    accounts: &[AccountView],
    args: &TransactArgs,
) -> Result<TransactAccounts, ProgramError> {
    let [
        pool,
        vault,
        nullifier0,
        nullifier1,
        user,
        user_token,
        recipient_token,
        relayer_token,
        token_program,
        system_program,
        ..,
    ] = accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer(user)?;
    programs(token_program, system_program)?;

    // The pool: this program's, a pool's size, at the PDA its stored bump makes.
    writable(pool)?;
    if !pool.owned_by(program_id) {
        return Err(ProgramError::InvalidAccountOwner);
    }
    let (mint, pool_bump, vault_bump) = {
        let data = pool.try_borrow()?;
        let state = Pool::load(&data)?;
        (state.mint, state.bump, state.vault_bump)
    };
    // The bumps were found canonical by `initialize`, so deriving with them is
    // `seeds = [..], bump = pool.bump` without the curve check's syscall.
    if pool.address() != &Address::derive_address(&[POOL_SEED, &mint], Some(pool_bump), program_id)
    {
        return Err(ProgramError::InvalidSeeds);
    }

    // The vault: at its PDA, the pool's own token account of the pool's mint.
    writable(vault)?;
    if vault.address()
        != &Address::derive_address(&[VAULT_SEED, &mint], Some(vault_bump), program_id)
    {
        return Err(ProgramError::InvalidSeeds);
    }
    {
        let vault = token_account(vault, &mint)?;
        if vault.owner() != pool.address() {
            return Err(ProgramError::IncorrectAuthority);
        }
    }

    // The nullifiers: each at its canonical PDA (another bump would be a second
    // account for the same nullifier, a double spend), and not yet spent.
    let nullifier = |account: &AccountView, nf: &[u8; 32]| {
        writable(account)?;
        let (address, bump) =
            Address::find_program_address(&[NULLIFIER_SEED, &mint, nf], program_id);
        if account.address() != &address {
            return Err(ProgramError::InvalidSeeds);
        }
        if account.owned_by(program_id) {
            return Err(ProgramError::from(PoolError::NullifierSpent));
        }
        Ok((*account, bump))
    };
    let nullifiers = [
        nullifier(nullifier0, args.nf[0])?,
        nullifier(nullifier1, args.nf[1])?,
    ];

    // The optional token accounts: this program's address in a slot means none
    // (the convention Anchor clients use for an absent optional account).
    let optional = |account: &AccountView| -> Result<Option<AccountView>, ProgramError> {
        if account.address() == program_id {
            return Ok(None);
        }
        writable(account)?;
        token_account(account, &mint)?;
        Ok(Some(*account))
    };
    Ok(TransactAccounts {
        pool: *pool,
        vault: *vault,
        nullifiers,
        user: *user,
        user_token: optional(user_token)?,
        recipient_token: optional(recipient_token)?,
        relayer_token: optional(relayer_token)?,
        mint,
        pool_bump,
    })
}

// ---- account checks ----------------------------------------------------------------

/// A writable signer: the payer of a new account.
fn signer(account: &AccountView) -> ProgramResult {
    if !account.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }
    writable(account)
}

fn writable(account: &AccountView) -> ProgramResult {
    if account.is_writable() {
        Ok(())
    } else {
        Err(ProgramError::Immutable)
    }
}

/// The token program and the system program, by address.
fn programs(token_program: &AccountView, system_program: &AccountView) -> ProgramResult {
    if token_program.address() != &pinocchio_token::ID
        || system_program.address() != &pinocchio_system::ID
    {
        return Err(ProgramError::IncorrectProgramId);
    }
    Ok(())
}

/// An initialized SPL token account (the token program's, of a token account's
/// size) of `mint`.
fn token_account<'a>(
    account: &'a AccountView,
    mint: &[u8; 32],
) -> Result<pinocchio::account::Ref<'a, TokenAccount>, ProgramError> {
    let token = TokenAccount::from_account_view(account)?;
    if !token.is_initialized() {
        return Err(ProgramError::UninitializedAccount);
    }
    if token.mint().as_array() != mint {
        return Err(PoolError::WrongMint.into());
    }
    Ok(token)
}

/// `account` is at the canonical PDA of `seeds` and not yet created (the system
/// program's and empty; it may hold lamports, which creation tops up). Returns the
/// bump.
fn fresh_pda(
    account: &AccountView,
    seeds: &[&[u8]],
    program_id: &Address,
) -> Result<u8, ProgramError> {
    writable(account)?;
    let (address, bump) = Address::find_program_address(seeds, program_id);
    if account.address() != &address {
        return Err(ProgramError::InvalidSeeds);
    }
    if !account.owned_by(&pinocchio_system::ID) || !account.is_data_empty() {
        return Err(ProgramError::AccountAlreadyInitialized);
    }
    Ok(bump)
}

// ---- state -------------------------------------------------------------------------

/// The pool's account data, 554 bytes, every field a byte array so the layout has
/// no padding and any alignment: `mint` at 0, `next_index` (u32 LE) at 32,
/// `current_root_index` (u32 LE) at 36, the ring of `roots` at 40, `bump` at 552,
/// `vault_bump` at 553.
#[repr(C)]
pub struct Pool {
    pub mint: [u8; 32],
    pub next_index: [u8; 4],
    pub current_root_index: [u8; 4],
    pub roots: [[u8; 32]; ROOT_HISTORY_SIZE],
    pub bump: u8,
    pub vault_bump: u8,
}

const _: () = assert!(core::mem::align_of::<Pool>() == 1);

impl Pool {
    pub const LEN: usize = core::mem::size_of::<Pool>();

    fn load(data: &[u8]) -> Result<&Pool, ProgramError> {
        if data.len() != Self::LEN {
            return Err(ProgramError::InvalidAccountData);
        }
        // SAFETY: `Pool` is `repr(C)`, of byte arrays only (alignment 1, no
        // padding, every bit pattern valid), and `data` is exactly its size.
        Ok(unsafe { &*(data.as_ptr() as *const Pool) })
    }

    fn load_mut(data: &mut [u8]) -> Result<&mut Pool, ProgramError> {
        if data.len() != Self::LEN {
            return Err(ProgramError::InvalidAccountData);
        }
        // SAFETY: as in `load`, and the borrow is unique.
        Ok(unsafe { &mut *(data.as_mut_ptr() as *mut Pool) })
    }

    fn current_root(&self) -> &[u8; 32] {
        &self.roots[u32::from_le_bytes(self.current_root_index) as usize % ROOT_HISTORY_SIZE]
    }

    /// Remember `root` as the newest ring entry; the tree grew by one pair.
    fn push_root(&mut self, root: &[u8; 32]) -> ProgramResult {
        let index = (u32::from_le_bytes(self.current_root_index) + 1) % ROOT_HISTORY_SIZE as u32;
        self.current_root_index = index.to_le_bytes();
        self.roots[index as usize] = *root;
        let next = u32::from_le_bytes(self.next_index)
            .checked_add(2)
            .ok_or(ProgramError::ArithmeticOverflow)?;
        self.next_index = next.to_le_bytes();
        Ok(())
    }

    /// Is `root` one of the recent ring entries? (All-zero never matches.)
    fn knows_root(&self, root: &[u8; 32]) -> bool {
        root.iter().any(|&b| b != 0) && self.roots.iter().any(|r| r == root)
    }
}

// ---- errors ------------------------------------------------------------------------

/// The program's own errors, `ProgramError::Custom(code)`; each logs its name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum PoolError {
    /// The proof doesn't verify against the public inputs assembled here.
    InvalidProof = 0,
    /// Not one of the last `ROOT_HISTORY_SIZE` roots.
    UnknownRoot = 1,
    TreeFull = 2,
    /// A deposit (`vpub_in > 0`) needs `user_token`.
    NoSource = 3,
    /// A withdrawal (`vpub_out > 0`) needs `recipient_token`.
    NoRecipient = 4,
    /// A fee (`fee > 0`) needs `relayer_token`.
    NoRelayer = 5,
    /// The nullifier's account exists: the note was spent.
    NullifierSpent = 6,
    /// A token account of another mint than the pool's.
    WrongMint = 7,
}

impl PoolError {
    fn name(self) -> &'static str {
        match self {
            Self::InvalidProof => "InvalidProof",
            Self::UnknownRoot => "UnknownRoot",
            Self::TreeFull => "TreeFull",
            Self::NoSource => "NoSource",
            Self::NoRecipient => "NoRecipient",
            Self::NoRelayer => "NoRelayer",
            Self::NullifierSpent => "NullifierSpent",
            Self::WrongMint => "WrongMint",
        }
    }
}

impl From<PoolError> for ProgramError {
    fn from(e: PoolError) -> Self {
        log(e.name());
        ProgramError::Custom(e as u32)
    }
}

// ---- logs --------------------------------------------------------------------------

fn log(message: &str) {
    #[cfg(any(target_os = "solana", target_arch = "bpf"))]
    // SAFETY: the pointer and length are a live `&str`'s.
    unsafe {
        pinocchio::syscalls::sol_log_(message.as_ptr(), message.len() as u64)
    };
    #[cfg(not(any(target_os = "solana", target_arch = "bpf")))]
    let _ = message;
}

/// One output note's event, as `sol_log_data` (`Program data:` in the logs, each
/// field base64): the leaf index (u32 LE), the note commitment, and the encrypted
/// memo the recipient trial-decrypts.
fn memo_event(leaf_index: u32, commitment: &[u8; 32], memo: &[u8]) {
    let index = leaf_index.to_le_bytes();
    let fields: [&[u8]; 3] = [&index, commitment, memo];
    #[cfg(any(target_os = "solana", target_arch = "bpf"))]
    // SAFETY: `fields` is an array of live slices, as `sol_log_data` reads them.
    unsafe {
        pinocchio::syscalls::sol_log_data(fields.as_ptr() as *const u8, fields.len() as u64)
    };
    #[cfg(not(any(target_os = "solana", target_arch = "bpf")))]
    let _ = fields;
}

// ---- public inputs -----------------------------------------------------------------

/// Does `args.proof` verify against the public inputs assembled here? Its own frame:
/// the inputs and the pairing check's locals don't fit beside `transact`'s in SBF's
/// 4 KiB.
#[inline(never)]
fn proves(pool: &Pool, args: &TransactArgs, recipient: &[u8; 32], relayer: &[u8; 32]) -> bool {
    verify(
        &Proof::from_le_bytes(args.proof),
        &public_inputs(pool, args, recipient, relayer),
    )
}

/// The transact circuit's public `Transaction`, field by field, in its order
/// (`xark inspect --json`).
fn public_inputs(
    pool: &Pool,
    args: &TransactArgs,
    recipient: &[u8; 32],
    relayer: &[u8; 32],
) -> [[u8; 32]; NUM_PUBLIC_INPUTS] {
    let [asset_hi, asset_lo] = halves(&pool.mint);
    let [recipient_hi, recipient_lo] = halves(recipient);
    let [relayer_hi, relayer_lo] = halves(relayer);
    let [memo0_hi, memo0_lo] = halves(&hash(args.memos[0]).to_bytes());
    let [memo1_hi, memo1_lo] = halves(&hash(args.memos[1]).to_bytes());
    [
        *args.root,
        asset_hi,
        asset_lo,
        *args.nf[0],
        *args.nf[1],
        *args.cm_out[0],
        *args.cm_out[1],
        *pool.current_root(),
        *args.new_root,
        field_from_u64(u64::from(u32::from_le_bytes(pool.next_index))),
        flow(args.vpub_in, args.vpub_out, args.fee),
        recipient_hi,
        recipient_lo,
        relayer_hi,
        relayer_lo,
        memo0_hi,
        memo0_lo,
        memo1_hi,
        memo1_lo,
    ]
}

/// A u64 as a 32-byte little-endian field element.
fn field_from_u64(x: u64) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[..8].copy_from_slice(&x.to_le_bytes());
    out
}

/// `Flow`'s three amounts as the circuit packs them, into one field element in
/// declaration order: `vpub_in + vpub_out·2⁶⁴ + fee·2¹²⁸`, little-endian.
fn flow(vpub_in: u64, vpub_out: u64, fee: u64) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[..8].copy_from_slice(&vpub_in.to_le_bytes());
    out[8..16].copy_from_slice(&vpub_out.to_le_bytes());
    out[16..24].copy_from_slice(&fee.to_le_bytes());
    out
}

/// 32 bytes as the circuit takes an `Address` or a `Hash`: `[hi, lo]`, each 16-byte
/// half read as a big-endian integer, as 32-byte little-endian field elements.
fn halves(bytes: &[u8; 32]) -> [[u8; 32]; 2] {
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
