//! Shielded transfer — the on-chain half (Anchor + SPL).
//!
//! One `transact` instruction covers deposit, private transfer, and withdraw
//! via the JoinSplit value slots. The chain never hashes: it verifies the
//! proof, checks the anchor root is remembered, burns the two nullifiers,
//! advances the tree to `new_root`, moves SPL for the public value slots, and
//! emits the two proof-bound encrypted memos for scanning wallets.
//!
//! Verifier calldata is the 256-byte proof followed by the circuit's 18 public
//! inputs, each a 32-byte little-endian field element, in declaration order.
//!
//! Reference implementation — unaudited, educational. See the README.
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

use shielded_transfer_transact_xark_verifier as verifier;
use solana_sha256_hasher::hash;

declare_id!("CUCcJJBRbK6tK4nPP2zgmvdYbKWSCGGVdRegtJ2GPJtF");

const TREE_HEIGHT: u32 = 20;
const ROOT_HISTORY_SIZE: usize = 16;
const PROOF_LEN: usize = 256;

const POOL_SEED: &[u8] = b"pool";
const VAULT_SEED: &[u8] = b"vault";
const NULLIFIER_SEED: &[u8] = b"nullifier";

// Root of an all-empty height-20 tree under the circuit's Poseidon2, LE.
// Regenerate with `cargo run --bin transfer-witness -- empty-root` (e2e crate).
const EMPTY_ROOT: [u8; 32] = [
    251, 142, 67, 247, 199, 111, 241, 124, 245, 96, 180, 214, 179, 51, 248, 149, 20, 177, 173, 83,
    193, 126, 124, 38, 168, 0, 139, 11, 55, 225, 46, 3,
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
        let pool = &ctx.accounts.pool;
        require!(proof.len() == PROOF_LEN, PoolError::BadProofLen);
        require!(
            (pool.next_index as u64 + 2) <= (1u64 << TREE_HEIGHT),
            PoolError::TreeFull
        );
        require!(pool.knows_root(&root), PoolError::UnknownRoot);
        // The circuit binds `fee` so a relayer flow can charge one, but this
        // program doesn't route fees anywhere — reject nonzero rather than
        // silently swallowing a public input.
        require!(fee == 0, PoolError::FeeNotSupported);

        let (recipient_hi, recipient_lo) =
            split_bytes(&ctx.accounts.recipient_token.key().to_bytes());
        let (memo0_hash_hi, memo0_hash_lo) = split_bytes(&hash(&memo0).to_bytes());
        let (memo1_hash_hi, memo1_hash_lo) = split_bytes(&hash(&memo1).to_bytes());

        // Public inputs in the transact circuit's order.
        let calldata = verifier_calldata(
            &proof,
            &[
                root,
                asset_from_mint(&pool.mint),
                nf[0],
                nf[1],
                cm_out[0],
                cm_out[1],
                pool.current_root(),
                new_root,
                field_from_u64(pool.next_index as u64),
                field_from_u64(vpub_in),
                field_from_u64(vpub_out),
                field_from_u64(fee),
                recipient_hi,
                recipient_lo,
                memo0_hash_hi,
                memo0_hash_lo,
                memo1_hash_hi,
                memo1_hash_lo,
            ],
        );
        require!(
            verifier::verify_instruction_data(&calldata),
            PoolError::InvalidProof
        );

        // Public value entering the shield: user → vault.
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

        // Public value leaving the shield: vault → recipient, signed by the pool PDA.
        if vpub_out > 0 {
            let (mint, bump) = (pool.mint, pool.bump);
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

        // Advance the tree past the two appended leaves and publish the memos
        // (delivery is off-circuit; wallets trial-decrypt to find their notes).
        let index = ctx.accounts.pool.next_index;
        ctx.accounts.pool.push_root(new_root);
        emit!(MemoEvent {
            leaf_index: index,
            commitment: cm_out[0],
            encrypted_memo: memo0
        });
        emit!(MemoEvent {
            leaf_index: index + 1,
            commitment: cm_out[1],
            encrypted_memo: memo1
        });
        Ok(())
    }
}

// ---- accounts ----------------------------------------------------------------

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
    /// Source for `vpub_in` (deposit). Unused when `vpub_in` is 0.
    #[account(mut)]
    pub user_token: Box<Account<'info, TokenAccount>>,
    /// Destination for `vpub_out` (withdraw). Its full address is bound into
    /// the proof; unused when `vpub_out` is 0.
    #[account(mut)]
    pub recipient_token: Box<Account<'info, TokenAccount>>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

// ---- state -------------------------------------------------------------------

#[account]
#[derive(InitSpace)]
pub struct Pool {
    pub mint: Pubkey,
    pub next_index: u32,
    pub current_root_index: u32,
    pub roots: [[u8; 32]; ROOT_HISTORY_SIZE],
    pub bump: u8,
}

impl Pool {
    fn current_root(&self) -> [u8; 32] {
        self.roots[self.current_root_index as usize]
    }

    /// Remember `root` as the newest ring entry; the tree grew by one pair.
    fn push_root(&mut self, root: [u8; 32]) {
        self.current_root_index = (self.current_root_index + 1) % ROOT_HISTORY_SIZE as u32;
        self.roots[self.current_root_index as usize] = root;
        self.next_index += 2;
    }

    /// Is `root` one of the recent ring entries? (All-zero never matches.)
    fn knows_root(&self, root: &[u8; 32]) -> bool {
        root.iter().any(|&b| b != 0) && self.roots.iter().any(|r| r == root)
    }
}

#[account]
pub struct Nullifier {}

/// One per output note; `ciphertext` is the off-circuit encrypted memo the
/// recipient trial-decrypts.
#[event]
pub struct MemoEvent {
    pub leaf_index: u32,
    pub commitment: [u8; 32],
    pub encrypted_memo: Vec<u8>,
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
    #[msg("fee routing is not implemented; pass fee = 0")]
    FeeNotSupported,
}

// ---- verifier plumbing ---------------------------------------------------------

/// proof ‖ public inputs — the exact byte string the generated verifier checks.
fn verifier_calldata(proof: &[u8], public_inputs: &[[u8; 32]]) -> Vec<u8> {
    let mut data = Vec::with_capacity(proof.len() + 32 * public_inputs.len());
    data.extend_from_slice(proof);
    for input in public_inputs {
        data.extend_from_slice(input);
    }
    data
}

/// A u64 as a 32-byte little-endian field element.
fn field_from_u64(x: u64) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[..8].copy_from_slice(&x.to_le_bytes());
    out
}

/// Split any 32-byte value into two 128-bit field elements:
/// `lo` = bytes[0..16], `hi` = bytes[16..32].
fn split_bytes(bytes: &[u8; 32]) -> ([u8; 32], [u8; 32]) {
    let mut hi = [0u8; 32];
    let mut lo = [0u8; 32];
    lo[..16].copy_from_slice(&bytes[..16]);
    hi[..16].copy_from_slice(&bytes[16..]);
    (hi, lo)
}

/// The circuit's per-token asset tag: the low 16 bytes of the mint pubkey
/// (< 2^128, so it always fits the field).
fn asset_from_mint(mint: &Pubkey) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[..16].copy_from_slice(&mint.to_bytes()[..16]);
    out
}
