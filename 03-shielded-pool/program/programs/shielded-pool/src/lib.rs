//! Shielded pool — on-chain program (Anchor).
//!
//! The chain never hashes. It stores only a ring of recent Merkle roots, a
//! `next_index`, and one marker account per spent nullifier. All Poseidon
//! lives in the two Noir circuits (`deposit`, `withdraw`); the program's whole
//! job is to verify their proofs (via the crates `xark export` generated) and
//! move lamports.
//!
//! Wire format reminder: xark public inputs are 32-byte **little-endian** field
//! elements. Each `instruction_data` we build is `proof (256 B) || public
//! inputs`, in the exact order the corresponding circuit declares them.
//!
//! ⚠️ Reference implementation — unaudited, educational. Do not deploy to
//! mainnet as-is. See ../../../README.md.
use anchor_lang::prelude::*;
use anchor_lang::system_program;

use shielded_pool_deposit_xark_verifier as deposit_verifier;
use shielded_pool_withdraw_xark_verifier as withdraw_verifier;

declare_id!("Sh1e1dedPoo1111111111111111111111111111111");

const ROOT_HISTORY_SIZE: usize = 64;
const TREE_HEIGHT: u32 = 20;
const FR: usize = 32;
const PROOF_LEN: usize = 256;

#[program]
pub mod shielded_pool {
    use super::*;

    /// Create the pool. `empty_root` is the root of an all-empty tree of height
    /// `TREE_HEIGHT` — i.e. `zeros[H]` from the deposit circuit. Compute it once
    /// off-chain (the client prints it) and pass it in.
    pub fn initialize(ctx: Context<Initialize>, denomination: u64, empty_root: [u8; 32]) -> Result<()> {
        let pool = &mut ctx.accounts.pool;
        pool.authority = ctx.accounts.authority.key();
        pool.denomination = denomination;
        pool.next_index = 0;
        pool.current_root_index = 0;
        pool.roots = [[0u8; 32]; ROOT_HISTORY_SIZE];
        pool.roots[0] = empty_root;
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
        proof: Vec<u8>,
    ) -> Result<()> {
        require!(proof.len() == PROOF_LEN, PoolError::BadProofLen);

        let (denomination, current_root_index, next_index) = {
            let p = &ctx.accounts.pool;
            (p.denomination, p.current_root_index, p.next_index)
        };
        require!((next_index as u64) < (1u64 << TREE_HEIGHT), PoolError::TreeFull);

        let old_root = ctx.accounts.pool.roots[current_root_index as usize];
        let index_le = u32_to_field_le(next_index);

        // public inputs order (deposit circuit): old_root, new_root, leaf, index
        let mut data = Vec::with_capacity(PROOF_LEN + 4 * FR);
        data.extend_from_slice(&proof);
        data.extend_from_slice(&old_root);
        data.extend_from_slice(&new_root);
        data.extend_from_slice(&commitment);
        data.extend_from_slice(&index_le);
        require!(
            deposit_verifier::verify_instruction_data(&data),
            PoolError::InvalidProof
        );

        // Pull the fixed denomination from the depositor into the pool PDA.
        system_program::transfer(
            CpiContext::new(
                ctx.accounts.system_program.to_account_info(),
                system_program::Transfer {
                    from: ctx.accounts.depositor.to_account_info(),
                    to: ctx.accounts.pool.to_account_info(),
                },
            ),
            denomination,
        )?;

        // Advance the tree: remember the new root, bump the index.
        let pool = &mut ctx.accounts.pool;
        let next = (current_root_index + 1) % (ROOT_HISTORY_SIZE as u32);
        pool.current_root_index = next;
        pool.roots[next as usize] = new_root;
        pool.next_index = next_index + 1;

        emit!(DepositEvent { commitment, index: next_index });
        Ok(())
    }

    /// Withdraw the denomination (minus `fee`) to `recipient`, paying `fee` to
    /// the relayer. The proof binds recipient/relayer/fee, so nobody can
    /// re-target it. The `nullifier` account is created here; a second attempt
    /// with the same nullifier fails at account creation → no double-spend.
    pub fn withdraw(
        ctx: Context<Withdraw>,
        proof: Vec<u8>,
        root: [u8; 32],
        nullifier_hash: [u8; 32],
        fee: u64,
    ) -> Result<()> {
        require!(proof.len() == PROOF_LEN, PoolError::BadProofLen);

        let denomination = ctx.accounts.pool.denomination;
        require!(fee <= denomination, PoolError::FeeTooHigh);
        require!(
            is_known_root(&ctx.accounts.pool, &root),
            PoolError::UnknownRoot
        );

        let (r_hi, r_lo) = split_pubkey(&ctx.accounts.recipient.key());
        let (l_hi, l_lo) = split_pubkey(&ctx.accounts.relayer.key());
        let fee_le = u64_to_field_le(fee);

        // public inputs order (withdraw circuit):
        //   root, nullifier_hash, recipient_hi, recipient_lo, relayer_hi, relayer_lo, fee
        let mut data = Vec::with_capacity(PROOF_LEN + 7 * FR);
        data.extend_from_slice(&proof);
        data.extend_from_slice(&root);
        data.extend_from_slice(&nullifier_hash);
        data.extend_from_slice(&r_hi);
        data.extend_from_slice(&r_lo);
        data.extend_from_slice(&l_hi);
        data.extend_from_slice(&l_lo);
        data.extend_from_slice(&fee_le);
        require!(
            withdraw_verifier::verify_instruction_data(&data),
            PoolError::InvalidProof
        );

        // Pay out from the pool PDA (program-owned → move lamports directly).
        let payout = denomination - fee;
        let pool_ai = ctx.accounts.pool.to_account_info();
        let recipient_ai = ctx.accounts.recipient.to_account_info();
        let relayer_ai = ctx.accounts.relayer.to_account_info();
        **pool_ai.try_borrow_mut_lamports()? -= denomination;
        **recipient_ai.try_borrow_mut_lamports()? += payout;
        **relayer_ai.try_borrow_mut_lamports()? += fee;
        Ok(())
    }
}

// ---- Accounts ---------------------------------------------------------------

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(init, payer = authority, space = 8 + Pool::SIZE, seeds = [b"pool"], bump)]
    pub pool: Account<'info, Pool>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(mut, seeds = [b"pool"], bump = pool.bump)]
    pub pool: Account<'info, Pool>,
    #[account(mut)]
    pub depositor: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(proof: Vec<u8>, root: [u8; 32], nullifier_hash: [u8; 32])]
pub struct Withdraw<'info> {
    #[account(mut, seeds = [b"pool"], bump = pool.bump)]
    pub pool: Account<'info, Pool>,
    /// Marker account; creation fails if this nullifier was already spent.
    #[account(
        init,
        payer = relayer,
        space = 8,
        seeds = [b"nullifier", nullifier_hash.as_ref()],
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

// ---- State ------------------------------------------------------------------

#[account]
pub struct Pool {
    pub authority: Pubkey,
    pub denomination: u64,
    pub next_index: u32,
    pub current_root_index: u32,
    pub roots: [[u8; 32]; ROOT_HISTORY_SIZE],
    pub bump: u8,
}

impl Pool {
    pub const SIZE: usize = 32 + 8 + 4 + 4 + (32 * ROOT_HISTORY_SIZE) + 1;
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
    #[msg("proof must be 256 bytes")]
    BadProofLen,
    #[msg("invalid proof")]
    InvalidProof,
    #[msg("unknown or stale merkle root")]
    UnknownRoot,
    #[msg("fee exceeds denomination")]
    FeeTooHigh,
    #[msg("tree is full")]
    TreeFull,
}

// ---- Helpers ----------------------------------------------------------------

fn is_known_root(pool: &Pool, root: &[u8; 32]) -> bool {
    if root.iter().all(|&b| b == 0) {
        return false;
    }
    pool.roots.iter().any(|r| r == root)
}

fn u32_to_field_le(x: u32) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[..4].copy_from_slice(&x.to_le_bytes());
    out
}

fn u64_to_field_le(x: u64) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[..8].copy_from_slice(&x.to_le_bytes());
    out
}

/// Split a 32-byte pubkey into two 128-bit field elements.
/// `lo` = bytes[0..16], `hi` = bytes[16..32], each little-endian in a 32-byte
/// field. The client feeds the circuit the identical split.
fn split_pubkey(pk: &Pubkey) -> ([u8; 32], [u8; 32]) {
    let b = pk.to_bytes();
    let mut hi = [0u8; 32];
    let mut lo = [0u8; 32];
    lo[..16].copy_from_slice(&b[..16]);
    hi[..16].copy_from_slice(&b[16..32]);
    (hi, lo)
}
