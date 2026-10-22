//! Shielded transfer — the on-chain half (Anchor + SPL).
//!
//! One `transact` instruction covers deposit, private transfer, and withdraw
//! via the JoinSplit value slots. The chain never hashes: it verifies the
//! proof, checks the anchor root is remembered, burns the two nullifiers,
//! advances the tree to `new_root`, moves SPL for the public value slots, and
//! emits the two proof-bound encrypted memos for scanning wallets.
//!
//! The proof is checked against the circuit's 21 public inputs, which the program
//! assembles itself from its state and the transaction: 32-byte little-endian
//! field elements, in the circuit's order.
//!
//! Reference implementation — unaudited, educational. See the README.
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

use shielded_transfer_transact_xark_verifier::{verify, Proof};
use solana_sha256_hasher::hash;

declare_id!("CUCcJJBRbK6tK4nPP2zgmvdYbKWSCGGVdRegtJ2GPJtF");

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
        proof: [u8; 256],
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
        require!(
            (pool.next_index as u64 + 2) <= (1u64 << TREE_HEIGHT),
            PoolError::TreeFull
        );
        require!(pool.knows_root(&root), PoolError::UnknownRoot);

        // The transact circuit's public `Transaction`, field by field.
        let [asset_hi, asset_lo] = halves(&pool.mint.to_bytes());
        // Without a withdrawal there's no recipient, and without a fee no relayer:
        // the proof binds this program's address in their place.
        let address = |a: &Option<Box<Account<TokenAccount>>>| {
            halves(&a.as_ref().map_or(crate::ID, |a| a.key()).to_bytes())
        };
        let [recipient_hi, recipient_lo] = address(&ctx.accounts.recipient_token);
        let [relayer_hi, relayer_lo] = address(&ctx.accounts.relayer_token);
        let [memo0_hi, memo0_lo] = halves(&hash(&memo0).to_bytes());
        let [memo1_hi, memo1_lo] = halves(&hash(&memo1).to_bytes());
        let public_inputs = [
            root,
            asset_hi,
            asset_lo,
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
            relayer_hi,
            relayer_lo,
            memo0_hi,
            memo0_lo,
            memo1_hi,
            memo1_lo,
        ];
        require!(
            verify(&Proof::from_le_bytes(&proof), &public_inputs),
            PoolError::InvalidProof
        );

        // Public value entering the shield: user → vault.
        if vpub_in > 0 {
            let user_token = ctx
                .accounts
                .user_token
                .as_ref()
                .ok_or(PoolError::NoSource)?;
            token::transfer(
                CpiContext::new(
                    ctx.accounts.token_program.key(),
                    Transfer {
                        from: user_token.to_account_info(),
                        to: ctx.accounts.vault.to_account_info(),
                        authority: ctx.accounts.user.to_account_info(),
                    },
                ),
                vpub_in,
            )?;
        }

        // Public value leaving the shield, signed by the pool PDA: `vpub_out` to the
        // recipient, `fee` to the relayer.
        let (mint, bump) = (pool.mint, pool.bump);
        let signer: &[&[&[u8]]] = &[&[POOL_SEED, mint.as_ref(), &[bump]]];
        let payouts = [
            (
                vpub_out,
                &ctx.accounts.recipient_token,
                PoolError::NoRecipient,
            ),
            (fee, &ctx.accounts.relayer_token, PoolError::NoRelayer),
        ];
        for (amount, to, missing) in payouts {
            if amount > 0 {
                let to = to.as_ref().ok_or(missing)?;
                token::transfer(
                    CpiContext::new_with_signer(
                        ctx.accounts.token_program.key(),
                        Transfer {
                            from: ctx.accounts.vault.to_account_info(),
                            to: to.to_account_info(),
                            authority: ctx.accounts.pool.to_account_info(),
                        },
                        signer,
                    ),
                    amount,
                )?;
            }
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
#[instruction(proof: [u8; 256], root: [u8; 32], nf: [[u8; 32]; 2])]
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
    /// Source for `vpub_in` (a deposit); left out when `vpub_in` is 0.
    #[account(mut)]
    pub user_token: Option<Box<Account<'info, TokenAccount>>>,
    /// Destination for `vpub_out` (a withdrawal), its address bound into the
    /// proof; left out when `vpub_out` is 0.
    #[account(mut)]
    pub recipient_token: Option<Box<Account<'info, TokenAccount>>>,
    /// Destination for `fee`: the token account of whoever relays the transaction,
    /// its address bound into the proof; left out when `fee` is 0.
    #[account(mut)]
    pub relayer_token: Option<Box<Account<'info, TokenAccount>>>,

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
    #[msg("invalid proof")]
    InvalidProof,
    #[msg("unknown or stale merkle root")]
    UnknownRoot,
    #[msg("tree is full")]
    TreeFull,
    #[msg("a deposit (vpub_in > 0) needs user_token")]
    NoSource,
    #[msg("a withdrawal (vpub_out > 0) needs recipient_token")]
    NoRecipient,
    #[msg("a fee (fee > 0) needs relayer_token")]
    NoRelayer,
}

// ---- public inputs -------------------------------------------------------------

/// A u64 as a 32-byte little-endian field element.
fn field_from_u64(x: u64) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[..8].copy_from_slice(&x.to_le_bytes());
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
