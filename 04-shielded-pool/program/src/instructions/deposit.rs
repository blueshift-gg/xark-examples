use pinocchio::{AccountView, ProgramResult, error::ProgramError};
use pinocchio_system::instructions::Transfer;
use shielded_pool_deposit_xark_verifier::{Proof, verify};

use crate::{
    ShieldedPoolError,
    helpers::{field_from_u64, log_data, system_program, writable_signer},
    state::{Pool, TREE_HEIGHT},
};

pub struct DepositAccounts<'a> {
    pub pool: &'a mut AccountView,
    pub depositor: &'a AccountView,
}

impl<'a> TryFrom<&'a mut [AccountView]> for DepositAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a mut [AccountView]) -> Result<Self, Self::Error> {
        let [pool, depositor, system, ..] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };
        Pool::check(pool)?;
        writable_signer(depositor)?;
        system_program(system)?;
        Ok(Self { pool, depositor })
    }
}

/// `commitment (32) ‖ new_root (32) ‖ proof (256)`.
pub struct DepositInstructionData<'a> {
    pub commitment: &'a [u8; 32],
    pub new_root: &'a [u8; 32],
    pub proof: &'a [u8; 256],
}

impl<'a> TryFrom<&'a [u8]> for DepositInstructionData<'a> {
    type Error = ProgramError;

    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        let (commitment, data) = data
            .split_first_chunk()
            .ok_or(ProgramError::InvalidInstructionData)?;
        let (new_root, proof) = data
            .split_first_chunk()
            .ok_or(ProgramError::InvalidInstructionData)?;
        let proof = proof
            .try_into()
            .map_err(|_| ProgramError::InvalidInstructionData)?;
        Ok(Self {
            commitment,
            new_root,
            proof,
        })
    }
}

/// Deposit the fixed denomination and append `commitment` to the tree. The proof
/// attests that `new_root` correctly extends the current tree by one leaf, so the
/// program can advance the tree without hashing.
pub struct Deposit<'a> {
    pub accounts: DepositAccounts<'a>,
    pub data: DepositInstructionData<'a>,
}

impl<'a> TryFrom<(&'a [u8], &'a mut [AccountView])> for Deposit<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a mut [AccountView])) -> Result<Self, Self::Error> {
        let accounts = DepositAccounts::try_from(accounts)?;
        let data = DepositInstructionData::try_from(data)?;
        Ok(Self { accounts, data })
    }
}

impl<'a> Deposit<'a> {
    pub const DISCRIMINATOR: &'a u8 = &1;

    pub fn process(&mut self) -> ProgramResult {
        let (denomination, index) = {
            let mut data = self.accounts.pool.try_borrow_mut()?;
            let pool = Pool::load_mut(&mut data)?;
            let index = pool.next_index();
            if u64::from(index) >= 1u64 << TREE_HEIGHT {
                return Err(ShieldedPoolError::TreeFull.into());
            }

            let public_inputs = deposit_inputs(
                pool.current_root(),
                self.data.new_root,
                self.data.commitment,
                index,
            );
            if !verify(&Proof::from_le_bytes(self.data.proof), &public_inputs) {
                return Err(ShieldedPoolError::InvalidProof.into());
            }

            // Advance the tree: remember the new root, bump the index.
            pool.push_root(self.data.new_root);
            (pool.denomination(), index)
        };

        // Pull the fixed denomination from the depositor into the pool PDA.
        Transfer {
            from: self.accounts.depositor,
            to: self.accounts.pool,
            lamports: denomination,
        }
        .invoke()?;

        // The event a wallet indexes the tree from: the commitment and its leaf index.
        log_data(&[self.data.commitment, &index.to_le_bytes()]);
        Ok(())
    }
}

/// The deposit circuit's public `Append`, field by field: `old_root, new_root,
/// leaf, index`.
pub fn deposit_inputs(
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
