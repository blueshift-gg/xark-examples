use pinocchio::{
    AccountView, Address, ProgramResult,
    cpi::{Seed, Signer},
    error::ProgramError,
};
use shielded_pool_withdraw_xark_verifier::{Proof, verify};

use crate::{
    ShieldedPoolError,
    helpers::{field_from_u64, is_uninitialized, system_program, writable, writable_signer},
    state::{NULLIFIER_SEED, Nullifier, Pool},
};

pub struct WithdrawAccounts<'a> {
    pub pool: &'a mut AccountView,
    pub nullifier: &'a mut AccountView,
    pub recipient: &'a mut AccountView,
    pub relayer: &'a mut AccountView,
}

impl<'a> TryFrom<&'a mut [AccountView]> for WithdrawAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a mut [AccountView]) -> Result<Self, Self::Error> {
        let [pool, nullifier, recipient, relayer, system, ..] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };
        Pool::check(pool)?;
        writable_signer(relayer)?;
        system_program(system)?;
        writable(recipient)?;
        // Only receives lamports, and its identity is bound into the proof; but never
        // one of the pool's own accounts, whose lamports the payout moves.
        if recipient.address() == pool.address() || recipient.address() == nullifier.address() {
            return Err(ProgramError::InvalidArgument);
        }
        writable(nullifier)?;
        Ok(Self {
            pool,
            nullifier,
            recipient,
            relayer,
        })
    }
}

/// `proof (256) ‖ root (32) ‖ nullifier_hash (32) ‖ fee (u64 LE)`.
pub struct WithdrawInstructionData<'a> {
    pub proof: &'a [u8; 256],
    pub root: &'a [u8; 32],
    pub nullifier_hash: &'a [u8; 32],
    pub fee: u64,
}

impl<'a> TryFrom<&'a [u8]> for WithdrawInstructionData<'a> {
    type Error = ProgramError;

    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        let (proof, data) = data
            .split_first_chunk()
            .ok_or(ProgramError::InvalidInstructionData)?;
        let (root, data) = data
            .split_first_chunk()
            .ok_or(ProgramError::InvalidInstructionData)?;
        let (nullifier_hash, fee) = data
            .split_first_chunk()
            .ok_or(ProgramError::InvalidInstructionData)?;
        let fee = fee
            .try_into()
            .map_err(|_| ProgramError::InvalidInstructionData)?;
        Ok(Self {
            proof,
            root,
            nullifier_hash,
            fee: u64::from_le_bytes(fee),
        })
    }
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
pub struct Withdraw<'a> {
    pub accounts: WithdrawAccounts<'a>,
    pub data: WithdrawInstructionData<'a>,
    pub denomination: [u8; 8],
    pub nullifier_bump: u8,
}

impl<'a> TryFrom<(&'a [u8], &'a mut [AccountView])> for Withdraw<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a mut [AccountView])) -> Result<Self, Self::Error> {
        let accounts = WithdrawAccounts::try_from(accounts)?;
        let data = WithdrawInstructionData::try_from(data)?;

        // The nullifier's marker account, namespaced by the denomination. It must sit
        // at the canonical bump: another bump would be another address for the same
        // note, and the note could be spent once at each.
        let denomination = Pool::load(&accounts.pool.try_borrow()?)?.denomination;
        let seeds = [NULLIFIER_SEED, &denomination, data.nullifier_hash];
        let (address, nullifier_bump) = Address::find_program_address(&seeds, &crate::ID);
        if accounts.nullifier.address() != &address {
            return Err(ProgramError::InvalidSeeds);
        }
        if !is_uninitialized(accounts.nullifier) {
            return Err(ShieldedPoolError::NullifierSpent.into());
        }
        Ok(Self {
            accounts,
            data,
            denomination,
            nullifier_bump,
        })
    }
}

impl<'a> Withdraw<'a> {
    pub const DISCRIMINATOR: &'a u8 = &2;

    pub fn process(&mut self) -> ProgramResult {
        let denomination = u64::from_le_bytes(self.denomination);
        let fee = self.data.fee;
        if fee > denomination {
            return Err(ShieldedPoolError::FeeTooHigh.into());
        }
        if !Pool::load(&self.accounts.pool.try_borrow()?)?.knows_root(self.data.root) {
            return Err(ShieldedPoolError::UnknownRoot.into());
        }

        let public_inputs = withdraw_inputs(
            self.data.root,
            self.data.nullifier_hash,
            self.accounts.recipient.address(),
            self.accounts.relayer.address(),
            fee,
        );
        if !verify(&Proof::from_le_bytes(self.data.proof), &public_inputs) {
            return Err(ShieldedPoolError::InvalidProof.into());
        }

        // Spend the note: create its marker, paid by the relayer.
        let bump = [self.nullifier_bump];
        let seeds = [
            Seed::from(NULLIFIER_SEED),
            Seed::from(&self.denomination),
            Seed::from(self.data.nullifier_hash),
            Seed::from(&bump),
        ];
        pinocchio_system::create_account_with_minimum_balance_signed(
            self.accounts.nullifier,
            Nullifier::LEN,
            &crate::ID,
            self.accounts.relayer,
            None,
            &[Signer::from(&seeds)],
        )?;
        self.accounts.nullifier.try_borrow_mut()?[0] = Nullifier::TAG;

        // Pay out from the pool PDA (program-owned, so its lamports move directly).
        // `fee <= denomination` above; the runtime keeps the pool rent-exempt. Each
        // move reads the balance it changes, so a recipient that is the relayer gets
        // both.
        debit(self.accounts.pool, denomination)?;
        credit(self.accounts.recipient, denomination - fee)?;
        credit(self.accounts.relayer, fee)
    }
}

pub fn debit(account: &mut AccountView, lamports: u64) -> ProgramResult {
    let balance = account
        .lamports()
        .checked_sub(lamports)
        .ok_or(ProgramError::InsufficientFunds)?;
    account.set_lamports(balance);
    Ok(())
}

pub fn credit(account: &mut AccountView, lamports: u64) -> ProgramResult {
    let balance = account
        .lamports()
        .checked_add(lamports)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    account.set_lamports(balance);
    Ok(())
}

/// The withdraw circuit's public `Withdrawal`, field by field: `root,
/// nullifier_hash, recipient (hi, lo), relayer (hi, lo), fee`. An address is wider
/// than the field, so it enters as its two halves (`Address`).
pub fn withdraw_inputs(
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
