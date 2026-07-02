//! Shielded transfer — on-chain program (Anchor + SPL). One `transact`
//! instruction covers deposit / private transfer / withdraw via the JoinSplit
//! value slots. The chain never hashes: it verifies the transact proof, checks
//! the anchor root is remembered, burns the two nullifiers, advances the tree
//! to `new_root`, moves SPL tokens for the public value slots, and emits the two
//! encrypted memos as events for the scanning wallet.
//!
//! Reference implementation — unaudited, educational.
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

use shielded_transfer_transact_xark_verifier as verifier;

declare_id!("CUCcJJBRbK6tK4nPP2zgmvdYbKWSCGGVdRegtJ2GPJtF");

const ROOT_HISTORY_SIZE: usize = 16;
const TREE_HEIGHT: u32 = 20;
const FR: usize = 32;
const PROOF_LEN: usize = 256;

const POOL_SEED: &[u8] = b"pool";
const VAULT_SEED: &[u8] = b"vault";
const NULLIFIER_SEED: &[u8] = b"nullifier";

// Root of an all-empty height-20 tree under this circuit's Poseidon2 sponge, LE.
const EMPTY_ROOT: [u8; 32] = [
    75, 160, 12, 16, 190, 37, 75, 229, 79, 138, 175, 76, 142, 198, 231, 254, 247, 25, 54, 200, 71,
    62, 71, 145, 25, 151, 120, 197, 135, 206, 3, 6,
];

#[program]
pub mod shielded_transfer {
    use super::*;

    /// Create the pool + token vault for an SPL mint (one pool per mint).
    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        let pool = &mut ctx.accounts.pool;
        pool.mint = ctx.accounts.mint.key();
        pool.next_index = 0;
        pool.current_root_index = 0;
        pool.roots[0] = EMPTY_ROOT;
        pool.bump = ctx.bumps.pool;
        Ok(())
    }

    /// One JoinSplit: spend 2 input notes, create 2 output notes, with public
    /// value entering (`vpub_in`) and/or leaving (`vpub_out`) the shield.
    #[allow(clippy::too_many_arguments)]
    pub fn transact(
        ctx: Context<Transact>,
        proof: Vec<u8>,
        root: [u8; 32],
        nf: [[u8; 32]; 2],
        cm_out: [[u8; 32]; 2],
        new_root: [u8; 32],
        vpub_in: u64,
        vpub_out: u64,
        fee: u64,
        memo0: Vec<u8>,
        memo1: Vec<u8>,
    ) -> Result<()> {
        require!(proof.len() == PROOF_LEN, PoolError::BadProofLen);

        let (mint, current_root_index, next_index, bump) = {
            let p = &ctx.accounts.pool;
            (p.mint, p.current_root_index, p.next_index, p.bump)
        };
        require!((next_index as u64 + 2) <= (1u64 << TREE_HEIGHT), PoolError::TreeFull);
        require!(is_known_root(&ctx.accounts.pool, &root), PoolError::UnknownRoot);

        let old_root = ctx.accounts.pool.roots[current_root_index as usize];
        let asset = asset_from_mint(&mint);

        // public inputs in the circuit's exact order (all LE 32-byte fields):
        // root, asset, nf0, nf1, cm0, cm1, old_root, new_root, index, vpub_in, vpub_out, fee
        let mut data = Vec::with_capacity(PROOF_LEN + 12 * FR);
        data.extend_from_slice(&proof);
        data.extend_from_slice(&root);
        data.extend_from_slice(&asset);
        data.extend_from_slice(&nf[0]);
        data.extend_from_slice(&nf[1]);
        data.extend_from_slice(&cm_out[0]);
        data.extend_from_slice(&cm_out[1]);
        data.extend_from_slice(&old_root);
        data.extend_from_slice(&new_root);
        data.extend_from_slice(&u32_field_le(next_index));
        data.extend_from_slice(&u64_field_le(vpub_in));
        data.extend_from_slice(&u64_field_le(vpub_out));
        data.extend_from_slice(&u64_field_le(fee));
        require!(verifier::verify_instruction_data(&data), PoolError::InvalidProof);

        // public value entering the shield: user -> vault
        if vpub_in > 0 {
            token::transfer(
                CpiContext::new(
                    ctx.accounts.token_program.key(),
                    Transfer {
                        from: ctx.accounts.user_token.to_account_info(),
                        to: ctx.accounts.vault.to_account_info(),
                        authority: ctx.accounts.user.to_account_info(),
                    },
                ),
                vpub_in,
            )?;
        }
        // public value leaving the shield: vault -> recipient (signed by pool PDA)
        if vpub_out > 0 {
            let signer: &[&[&[u8]]] = &[&[POOL_SEED, mint.as_ref(), &[bump]]];
            token::transfer(
                CpiContext::new_with_signer(
                    ctx.accounts.token_program.key(),
                    Transfer {
                        from: ctx.accounts.vault.to_account_info(),
                        to: ctx.accounts.recipient_token.to_account_info(),
                        authority: ctx.accounts.pool.to_account_info(),
                    },
                    signer,
                ),
                vpub_out,
            )?;
        }

        // advance the tree: two leaves appended, remember the final root
        let pool = &mut ctx.accounts.pool;
        let next = (current_root_index + 1) % (ROOT_HISTORY_SIZE as u32);
        pool.roots[next as usize] = new_root;
        pool.current_root_index = next;
        pool.next_index = next_index + 2;

        // publish encrypted memos for scanning (delivery is off-circuit)
        emit!(MemoEvent { leaf_index: next_index, commitment: cm_out[0], ciphertext: memo0 });
        emit!(MemoEvent { leaf_index: next_index + 1, commitment: cm_out[1], ciphertext: memo1 });
        let _ = fee;
        Ok(())
    }
}

// ---- Accounts ---------------------------------------------------------------

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + Pool::INIT_SPACE,
        seeds = [POOL_SEED, mint.key().as_ref()],
        bump
    )]
    pub pool: Box<Account<'info, Pool>>,
    #[account(
        init,
        payer = authority,
        seeds = [VAULT_SEED, mint.key().as_ref()],
        bump,
        token::mint = mint,
        token::authority = pool
    )]
    pub vault: Box<Account<'info, TokenAccount>>,
    pub mint: Box<Account<'info, Mint>>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
#[instruction(proof: Vec<u8>, root: [u8; 32], nf: [[u8; 32]; 2])]
pub struct Transact<'info> {
    #[account(mut, seeds = [POOL_SEED, pool.mint.as_ref()], bump = pool.bump)]
    pub pool: Box<Account<'info, Pool>>,
    #[account(mut, seeds = [VAULT_SEED, pool.mint.as_ref()], bump)]
    pub vault: Box<Account<'info, TokenAccount>>,

    // One marker account per spent nullifier; creation fails if already spent.
    #[account(init, payer = user, space = 8, seeds = [NULLIFIER_SEED, pool.mint.as_ref(), nf[0].as_ref()], bump)]
    pub nullifier0: Account<'info, Nullifier>,
    #[account(init, payer = user, space = 8, seeds = [NULLIFIER_SEED, pool.mint.as_ref(), nf[1].as_ref()], bump)]
    pub nullifier1: Account<'info, Nullifier>,

    #[account(mut)]
    pub user: Signer<'info>,
    // Source for vpub_in (deposit). Unused when vpub_in == 0.
    #[account(mut)]
    pub user_token: Box<Account<'info, TokenAccount>>,
    // Destination for vpub_out (withdraw). Unused when vpub_out == 0.
    /// CHECK: an SPL token account; identity is bound into the proof via the note.
    #[account(mut)]
    pub recipient_token: Box<Account<'info, TokenAccount>>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

// ---- State ------------------------------------------------------------------

#[account]
#[derive(InitSpace)]
pub struct Pool {
    pub mint: Pubkey,
    pub next_index: u32,
    pub current_root_index: u32,
    pub roots: [[u8; 32]; ROOT_HISTORY_SIZE],
    pub bump: u8,
}

#[account]
pub struct Nullifier {}

#[event]
pub struct MemoEvent {
    pub leaf_index: u32,
    pub commitment: [u8; 32],
    pub ciphertext: Vec<u8>,
}

#[error_code]
pub enum PoolError {
    #[msg("proof must be 256 bytes")]
    BadProofLen,
    #[msg("invalid proof")]
    InvalidProof,
    #[msg("unknown or stale merkle root")]
    UnknownRoot,
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

// asset field tag = low 16 bytes of the mint pubkey (< 2^128 < BN254 prime).
fn asset_from_mint(mint: &Pubkey) -> [u8; 32] {
    let b = mint.to_bytes();
    let mut out = [0u8; 32];
    out[..16].copy_from_slice(&b[..16]);
    out
}

fn u32_field_le(x: u32) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[..4].copy_from_slice(&x.to_le_bytes());
    out
}
fn u64_field_le(x: u64) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[..8].copy_from_slice(&x.to_le_bytes());
    out
}
