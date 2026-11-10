use pinocchio::{
    AccountView, ProgramResult,
    error::ProgramError,
    sysvars::{Sysvar, clock::Clock},
};
use zkyc_xark_verifier::{Proof, verify};

use crate::{ZkycError, policy::public_inputs};

pub struct VerifyKycAccounts<'a> {
    pub holder: &'a AccountView,
}

impl<'a> TryFrom<&'a [AccountView]> for VerifyKycAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountView]) -> Result<Self, Self::Error> {
        let [holder, ..] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };
        // The proof is bound to this address, so only its owner can present it.
        if !holder.is_signer() {
            return Err(ProgramError::MissingRequiredSignature);
        }
        Ok(Self { holder })
    }
}

/// `proof (256 B) ‖ commitment (32 B, little-endian)`.
pub struct VerifyKycInstructionData<'a> {
    pub proof: &'a [u8; 256],
    pub commitment: &'a [u8; 32],
}

impl<'a> TryFrom<&'a [u8]> for VerifyKycInstructionData<'a> {
    type Error = ProgramError;

    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        let (proof, commitment) = data
            .split_first_chunk::<256>()
            .ok_or(ProgramError::InvalidInstructionData)?;
        let commitment = commitment
            .try_into()
            .map_err(|_| ProgramError::InvalidInstructionData)?;
        Ok(Self { proof, commitment })
    }
}

pub struct VerifyKyc<'a> {
    pub accounts: VerifyKycAccounts<'a>,
    pub data: VerifyKycInstructionData<'a>,
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountView])> for VerifyKyc<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a [AccountView])) -> Result<Self, Self::Error> {
        let accounts = VerifyKycAccounts::try_from(accounts)?;
        let data = VerifyKycInstructionData::try_from(data)?;
        Ok(Self { accounts, data })
    }
}

impl<'a> VerifyKyc<'a> {
    pub const DISCRIMINATOR: &'a u8 = &0;

    pub fn process(&mut self) -> ProgramResult {
        // Today comes from the cluster, not the prover, so a proof can't be backdated.
        let unix = Clock::get()?.unix_timestamp;
        let public_inputs = public_inputs(
            self.data.commitment,
            unix,
            self.accounts.holder.address().as_array(),
        );
        if !verify(&Proof::from_le_bytes(self.data.proof), &public_inputs) {
            return Err(ZkycError::InvalidProof.into());
        }
        Ok(())
    }
}
