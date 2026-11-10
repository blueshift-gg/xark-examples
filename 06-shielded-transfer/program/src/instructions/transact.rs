use pinocchio::{
    AccountView, Address, ProgramResult,
    account::Ref,
    cpi::{Seed, Signer},
    error::ProgramError,
};
use pinocchio_system::create_account_with_minimum_balance_signed;
use pinocchio_token::{instructions::Transfer, state::Account as TokenAccount};
use shielded_transfer_transact_xark_verifier::{NUM_PUBLIC_INPUTS, Proof, verify};
use solana_sha256_hasher::hash;

use crate::{
    errors::ShieldedTransferError,
    helpers::{programs, signer, writable},
    state::{NULLIFIER_LEN, NULLIFIER_SEED, POOL_SEED, Pool, TREE_HEIGHT, VAULT_SEED},
};

/// The pool, its vault, the two nullifiers, the user who pays their rent, and the
/// token accounts the transaction moves value between, each optional.
pub struct TransactAccounts<'a> {
    pub pool: &'a mut AccountView,
    pub vault: &'a AccountView,
    pub nullifiers: [&'a mut AccountView; 2],
    pub user: &'a AccountView,
    pub user_token: Option<&'a AccountView>,
    pub recipient_token: Option<&'a AccountView>,
    pub relayer_token: Option<&'a AccountView>,
    /// From the pool's data.
    pub mint: [u8; 32],
    pub pool_bump: u8,
}

impl<'a> TryFrom<&'a mut [AccountView]> for TransactAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a mut [AccountView]) -> Result<Self, Self::Error> {
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

        writable(pool)?;
        let (mint, pool_bump, vault_bump) = {
            let state = Pool::from_account_view(pool)?;
            (state.mint, state.bump, state.vault_bump)
        };
        // `initialize` found the bumps canonical, so one derivation each checks the
        // address, without the search.
        if pool.address()
            != &Address::derive_address(&[POOL_SEED, &mint], Some(pool_bump), &crate::ID)
        {
            return Err(ProgramError::InvalidSeeds);
        }

        writable(vault)?;
        if vault.address()
            != &Address::derive_address(&[VAULT_SEED, &mint], Some(vault_bump), &crate::ID)
        {
            return Err(ProgramError::InvalidSeeds);
        }
        if token_account(vault, &mint)?.owner() != pool.address() {
            return Err(ProgramError::IncorrectAuthority);
        }

        // Their addresses depend on the instruction data: `Transact` checks those.
        writable(nullifier0)?;
        writable(nullifier1)?;

        // This program's address in a slot means none.
        let optional = |account: &'a AccountView| -> Result<Option<&'a AccountView>, ProgramError> {
            if account.address() == &crate::ID {
                return Ok(None);
            }
            writable(account)?;
            token_account(account, &mint)?;
            Ok(Some(account))
        };
        Ok(Self {
            user_token: optional(user_token)?,
            recipient_token: optional(recipient_token)?,
            relayer_token: optional(relayer_token)?,
            pool,
            vault,
            nullifiers: [nullifier0, nullifier1],
            user,
            mint,
            pool_bump,
        })
    }
}

/// An initialized SPL token account (the token program's, of a token account's
/// size) of `mint`.
fn token_account<'a>(
    account: &'a AccountView,
    mint: &[u8; 32],
) -> Result<Ref<'a, TokenAccount>, ProgramError> {
    let token = TokenAccount::from_account_view(account)?;
    if !token.is_initialized() {
        return Err(ProgramError::UninitializedAccount);
    }
    if token.mint().as_array() != mint {
        return Err(ShieldedTransferError::WrongMint.into());
    }
    Ok(token)
}

/// `proof` (256 B) ‖ `root` ‖ `nf[0]` ‖ `nf[1]` ‖ `cm_out[0]` ‖ `cm_out[1]` ‖
/// `new_root` (32 B each) ‖ `vpub_in` ‖ `vpub_out` ‖ `fee` (u64 LE each) ‖ `memo0` ‖
/// `memo1` (each a u32 LE length, then its bytes).
pub struct TransactInstructionData<'a> {
    pub proof: &'a [u8; 256],
    pub root: &'a [u8; 32],
    pub nf: [&'a [u8; 32]; 2],
    pub cm_out: [&'a [u8; 32]; 2],
    pub new_root: &'a [u8; 32],
    pub vpub_in: u64,
    pub vpub_out: u64,
    pub fee: u64,
    pub memos: [&'a [u8]; 2],
}

impl<'a> TryFrom<&'a [u8]> for TransactInstructionData<'a> {
    type Error = ProgramError;

    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
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

/// A memo: its length as a u32, then its bytes (Borsh's `Vec<u8>`).
fn memo<'a>(rest: &mut &'a [u8]) -> Result<&'a [u8], ProgramError> {
    let len = u32::from_le_bytes(*take::<4>(rest)?) as usize;
    let (memo, tail) = rest
        .split_at_checked(len)
        .ok_or(ProgramError::InvalidInstructionData)?;
    *rest = tail;
    Ok(memo)
}

/// One JoinSplit: spend 2 input notes, create 2 output notes, with public value
/// entering (`vpub_in`) and/or leaving (`vpub_out`, and the relayer's `fee`) the
/// shield.
pub struct Transact<'a> {
    pub accounts: TransactAccounts<'a>,
    pub data: TransactInstructionData<'a>,
    pub nullifier_bumps: [u8; 2],
}

impl<'a> TryFrom<(&'a [u8], &'a mut [AccountView])> for Transact<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a mut [AccountView])) -> Result<Self, Self::Error> {
        let data = TransactInstructionData::try_from(data)?;
        let accounts = TransactAccounts::try_from(accounts)?;

        // Each nullifier at its canonical PDA, found here rather than taken from the
        // client: another bump would be a second account for the same nullifier, a
        // double spend. Its account existing means the note is spent.
        let mut nullifier_bumps = [0; 2];
        for (i, nullifier) in accounts.nullifiers.iter().enumerate() {
            let (address, bump) = Address::find_program_address(
                &[NULLIFIER_SEED, &accounts.mint, data.nf[i]],
                &crate::ID,
            );
            if nullifier.address() != &address {
                return Err(ProgramError::InvalidSeeds);
            }
            if nullifier.owned_by(&crate::ID) {
                return Err(ShieldedTransferError::NullifierSpent.into());
            }
            nullifier_bumps[i] = bump;
        }
        Ok(Self {
            accounts,
            data,
            nullifier_bumps,
        })
    }
}

impl<'a> Transact<'a> {
    pub const DISCRIMINATOR: &'a u8 = &1;

    pub fn process(&mut self) -> ProgramResult {
        let (a, data) = (&mut self.accounts, &self.data);
        let mint = a.mint;

        let next_index = {
            let pool = Pool::from_account_view(a.pool)?;
            let next_index = u32::from_le_bytes(pool.next_index);
            if u64::from(next_index) + 2 > 1u64 << TREE_HEIGHT {
                return Err(ShieldedTransferError::TreeFull.into());
            }
            if !pool.knows_root(data.root) {
                return Err(ShieldedTransferError::UnknownRoot.into());
            }
            // Without a withdrawal there's no recipient, and without a fee no
            // relayer: the proof binds this program's address in their place.
            let address =
                |a: Option<&AccountView>| *a.map_or(&crate::ID, |a| a.address()).as_array();
            let (recipient, relayer) = (address(a.recipient_token), address(a.relayer_token));
            if !proves(&pool, data, &recipient, &relayer) {
                return Err(ShieldedTransferError::InvalidProof.into());
            }
            next_index
        };

        // Burn the nullifiers: one empty account each, paid for by the user.
        for (i, nullifier) in a.nullifiers.iter_mut().enumerate() {
            let bump = [self.nullifier_bumps[i]];
            let seeds = [
                Seed::from(NULLIFIER_SEED),
                Seed::from(&mint),
                Seed::from(data.nf[i]),
                Seed::from(&bump),
            ];
            create_account_with_minimum_balance_signed(
                nullifier,
                NULLIFIER_LEN,
                &crate::ID,
                a.user,
                None,
                &[Signer::from(&seeds)],
            )?;
        }

        // Public value entering the shield: user → vault, signed by the user.
        if data.vpub_in > 0 {
            let user_token = a.user_token.ok_or(ShieldedTransferError::NoSource)?;
            Transfer::new(user_token, a.vault, a.user, data.vpub_in).invoke()?;
        }

        // Public value leaving the shield, signed by the pool PDA: `vpub_out` to the
        // recipient, `fee` to the relayer. Their sum must fit the vault's u64 balance.
        data.vpub_out
            .checked_add(data.fee)
            .ok_or(ProgramError::ArithmeticOverflow)?;
        let bump = [a.pool_bump];
        let seeds = [Seed::from(POOL_SEED), Seed::from(&mint), Seed::from(&bump)];
        let payouts = [
            (
                data.vpub_out,
                a.recipient_token,
                ShieldedTransferError::NoRecipient,
            ),
            (data.fee, a.relayer_token, ShieldedTransferError::NoRelayer),
        ];
        for (amount, to, missing) in payouts {
            if amount > 0 {
                let to = to.ok_or(missing)?;
                Transfer::new(a.vault, to, a.pool, amount)
                    .invoke_signed(&[Signer::from(&seeds)])?;
            }
        }

        // Advance the tree past the two appended leaves and publish the memos
        // (delivery is off-circuit; wallets trial-decrypt to find their notes).
        Pool::from_account_view_mut(a.pool)?.push_root(data.new_root)?;
        for (i, memo) in data.memos.into_iter().enumerate() {
            memo_event(next_index + i as u32, data.cm_out[i], memo);
        }
        Ok(())
    }
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

/// Does `data.proof` verify against the public inputs assembled here? Its own
/// frame: the inputs and the pairing check's locals don't fit beside `process`'s in
/// SBF's 4 KiB.
#[inline(never)]
fn proves(
    pool: &Pool,
    data: &TransactInstructionData,
    recipient: &[u8; 32],
    relayer: &[u8; 32],
) -> bool {
    verify(
        &Proof::from_le_bytes(data.proof),
        &public_inputs(pool, data, recipient, relayer),
    )
}

/// The transact circuit's public `Transaction`, field by field, in its order
/// (`xark inspect --json`).
pub fn public_inputs(
    pool: &Pool,
    data: &TransactInstructionData,
    recipient: &[u8; 32],
    relayer: &[u8; 32],
) -> [[u8; 32]; NUM_PUBLIC_INPUTS] {
    let [asset_hi, asset_lo] = halves(&pool.mint);
    let [recipient_hi, recipient_lo] = halves(recipient);
    let [relayer_hi, relayer_lo] = halves(relayer);
    let [memo0_hi, memo0_lo] = halves(&hash(data.memos[0]).to_bytes());
    let [memo1_hi, memo1_lo] = halves(&hash(data.memos[1]).to_bytes());
    [
        *data.root,
        asset_hi,
        asset_lo,
        *data.nf[0],
        *data.nf[1],
        *data.cm_out[0],
        *data.cm_out[1],
        *pool.current_root(),
        *data.new_root,
        field_from_u64(u64::from(u32::from_le_bytes(pool.next_index))),
        flow(data.vpub_in, data.vpub_out, data.fee),
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
pub fn halves(bytes: &[u8; 32]) -> [[u8; 32]; 2] {
    let half = |b: &[u8]| {
        let mut out = [0u8; 32];
        out[..16].copy_from_slice(b);
        out[..16].reverse();
        out
    };
    [half(&bytes[..16]), half(&bytes[16..])]
}
