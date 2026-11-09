//! Shielded pool — the on-chain half (Anchor).
//!
//! The program never hashes. All Poseidon lives in the circuits; the chain
//! keeps only what consensus needs — a ring of recent Merkle roots, the next
//! leaf index, and one marker account per spent nullifier — and verifies two
//! proofs (via the crates `xark export` generated):
//!
//!   deposit   "new_root extends the current tree by one leaf"  → store new_root
//!   withdraw  "I own some leaf, here is its nullifier"          → pay out once
//!
//! Each proof is checked against public inputs the program assembles itself,
//! 32-byte little-endian field elements in the circuit's order, so a proof only
//! holds for the state and accounts of the transaction carrying it.
//!
//! Reference implementation — unaudited, educational. See the README.
use anchor_lang::prelude::*;
use anchor_lang::system_program;

use shielded_pool_deposit_xark_verifier as deposit_verifier;
use shielded_pool_deposit_xark_verifier::Proof;
use shielded_pool_withdraw_xark_verifier as withdraw_verifier; // the same type in both verifier crates

// Placeholder program id — replace with yours via `anchor keys sync`.
declare_id!("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS");

const TREE_HEIGHT: u32 = 20;
const ROOT_HISTORY_SIZE: usize = 16;

const POOL_SEED: &[u8] = b"pool";
const NULLIFIER_SEED: &[u8] = b"nullifier";

// The empty tree's root, little-endian: the first deposit's `old_root`. Fixed
// here so a pool can't be seeded with another; the client tests check it against
// the circuit's `Path.first().treeRoot()`.
const EMPTY_ROOT: [u8; 32] = [
    26, 122, 47, 146, 164, 115, 191, 166, 88, 234, 28, 252, 38, 169, 221, 171, 246, 159, 204, 103,
    2, 229, 199, 103, 212, 104, 254, 41, 151, 59, 127, 46,
];

#[program]
pub mod shielded_pool {
    use super::*;

    /// Create the pool for one denomination. Permissionless: the first caller
    /// fixes the size (fine for a demo); the empty-tree root is a program
    /// constant, so the pool cannot be seeded with a wrong root.
    pub fn initialize(ctx: Context<Initialize>, denomination: u64) -> Result<()> {
        let pool = &mut ctx.accounts.pool;
        pool.denomination = denomination;
        pool.next_index = 0;
        pool.current_root_index = 0;
        pool.roots[0] = EMPTY_ROOT; // `init` zeroed the rest of the ring
        pool.bump = ctx.bumps.pool;
        Ok(())
    }

    /// Deposit the fixed denomination and append `commitment` to the tree.
    /// The proof attests that `new_root` correctly extends the current tree by
    /// one leaf — so the program can advance the tree without hashing.
    pub fn deposit(
        ctx: Context<Deposit>,
        commitment: [u8; 32],
        new_root: [u8; 32],
        proof: [u8; 256],
    ) -> Result<()> {
        let pool = &ctx.accounts.pool;
        require!(
            (pool.next_index as u64) < (1u64 << TREE_HEIGHT),
            PoolError::TreeFull
        );

        // The deposit circuit's public `Append`, field by field.
        let public_inputs = [
            pool.current_root(),
            new_root,
            commitment,
            field_from_u64(pool.next_index as u64),
        ];
        require!(
            deposit_verifier::verify(&Proof::from_le_bytes(&proof), &public_inputs),
            PoolError::InvalidProof
        );

        // Pull the fixed denomination from the depositor into the pool PDA.
        system_program::transfer(
            CpiContext::new(
                ctx.accounts.system_program.key(),
                system_program::Transfer {
                    from: ctx.accounts.depositor.to_account_info(),
                    to: ctx.accounts.pool.to_account_info(),
                },
            ),
            pool.denomination,
        )?;

        // Advance the tree: remember the new root, bump the index.
        let index = ctx.accounts.pool.next_index;
        ctx.accounts.pool.push_root(new_root);
        emit!(DepositEvent { commitment, index });
        Ok(())
    }

    /// Withdraw the denomination (minus `fee`) to `recipient`, paying `fee` to
    /// the relayer. The proof binds recipient/relayer/fee, so nobody can
    /// re-target it. The `nullifier` account is created here; a second attempt
    /// with the same nullifier fails at account creation → no double-spend.
    ///
    /// Relayer economics: the relayer pays the (non-refundable) nullifier rent
    /// and the tx fee, and is compensated only by `fee`. `fee` is chosen by the
    /// depositor and bound into the proof, so there's no on-chain floor — a
    /// relayer must check `fee` covers its costs off-chain before submitting.
    pub fn withdraw(
        ctx: Context<Withdraw>,
        proof: [u8; 256],
        root: [u8; 32],
        nullifier_hash: [u8; 32],
        fee: u64,
    ) -> Result<()> {
        let pool = &ctx.accounts.pool;
        require!(fee <= pool.denomination, PoolError::FeeTooHigh);
        require!(pool.knows_root(&root), PoolError::UnknownRoot);

        // The withdraw circuit's public `Withdrawal`, field by field. An address is
        // wider than the field, so it enters as its two halves (`Address`).
        let [r_hi, r_lo] = address_halves(&ctx.accounts.recipient.key());
        let [l_hi, l_lo] = address_halves(&ctx.accounts.relayer.key());
        let public_inputs = [
            root,
            nullifier_hash,
            r_hi,
            r_lo,
            l_hi,
            l_lo,
            field_from_u64(fee),
        ];
        require!(
            withdraw_verifier::verify(&Proof::from_le_bytes(&proof), &public_inputs),
            PoolError::InvalidProof
        );

        // Pay out from the pool PDA (program-owned → move lamports directly).
        let denomination = pool.denomination;
        let pool_ai = ctx.accounts.pool.to_account_info();
        let recipient_ai = ctx.accounts.recipient.to_account_info();
        let relayer_ai = ctx.accounts.relayer.to_account_info();
        **pool_ai.try_borrow_mut_lamports()? -= denomination;
        **recipient_ai.try_borrow_mut_lamports()? += denomination - fee;
        **relayer_ai.try_borrow_mut_lamports()? += fee;
        Ok(())
    }
}

// ---- accounts ----------------------------------------------------------------

#[derive(Accounts)]
#[instruction(denomination: u64)]
pub struct Initialize<'info> {
    // One pool per denomination (like Tornado's separate per-size pools).
    #[account(
        init,
        payer = authority,
        space = 8 + Pool::INIT_SPACE,
        seeds = [POOL_SEED, denomination.to_le_bytes().as_ref()],
        bump
    )]
    pub pool: Box<Account<'info, Pool>>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(mut, seeds = [POOL_SEED, pool.denomination.to_le_bytes().as_ref()], bump = pool.bump)]
    pub pool: Box<Account<'info, Pool>>,
    #[account(mut)]
    pub depositor: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(proof: [u8; 256], root: [u8; 32], nullifier_hash: [u8; 32])]
pub struct Withdraw<'info> {
    #[account(mut, seeds = [POOL_SEED, pool.denomination.to_le_bytes().as_ref()], bump = pool.bump)]
    pub pool: Box<Account<'info, Pool>>,
    /// Marker account; creation fails if this nullifier was already spent.
    #[account(
        init,
        payer = relayer,
        space = 8,
        seeds = [NULLIFIER_SEED, pool.denomination.to_le_bytes().as_ref(), nullifier_hash.as_ref()],
        bump
    )]
    pub nullifier: Account<'info, Nullifier>,
    /// CHECK: only receives lamports; its identity is bound into the proof.
    #[account(mut)]
    pub recipient: UncheckedAccount<'info>,
    #[account(mut)]
    pub relayer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

// ---- state -------------------------------------------------------------------

#[account]
#[derive(InitSpace)]
pub struct Pool {
    pub denomination: u64,
    pub next_index: u32,
    pub current_root_index: u32,
    pub roots: [[u8; 32]; ROOT_HISTORY_SIZE],
    pub bump: u8,
}

impl Pool {
    fn current_root(&self) -> [u8; 32] {
        self.roots[self.current_root_index as usize]
    }

    /// Remember `root` as the newest entry in the ring and advance the tree.
    fn push_root(&mut self, root: [u8; 32]) {
        self.current_root_index = (self.current_root_index + 1) % ROOT_HISTORY_SIZE as u32;
        self.roots[self.current_root_index as usize] = root;
        self.next_index += 1;
    }

    /// Is `root` one of the recent ring entries? (All-zero never matches: the
    /// unused ring slots are zeroed, and a real root is never zero.)
    fn knows_root(&self, root: &[u8; 32]) -> bool {
        root.iter().any(|&b| b != 0) && self.roots.iter().any(|r| r == root)
    }
}

#[account]
pub struct Nullifier {}

#[event]
pub struct DepositEvent {
    pub commitment: [u8; 32],
    pub index: u32,
}

#[error_code]
pub enum PoolError {
    #[msg("invalid proof")]
    InvalidProof,
    #[msg("unknown or stale merkle root")]
    UnknownRoot,
    #[msg("fee exceeds denomination")]
    FeeTooHigh,
    #[msg("tree is full")]
    TreeFull,
}

// ---- public inputs -------------------------------------------------------------

/// A u64 as a 32-byte little-endian field element.
fn field_from_u64(x: u64) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[..8].copy_from_slice(&x.to_le_bytes());
    out
}

/// An address as the circuit's `Address`: `[hi, lo]`, each 16-byte half read as a
/// big-endian integer, as 32-byte little-endian field elements.
fn address_halves(address: &Pubkey) -> [[u8; 32]; 2] {
    let bytes = address.to_bytes();
    let half = |b: &[u8]| {
        let mut out = [0u8; 32];
        out[..16].copy_from_slice(b);
        out[..16].reverse();
        out
    };
    [half(&bytes[..16]), half(&bytes[16..])]
}
