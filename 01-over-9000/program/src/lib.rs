//! Onchain verifier for the "over 9000" circuit. It has one instruction and takes no
//! accounts: the instruction data is the 256-byte proof, as the circuit has no public
//! inputs. `verify` comes from the crate `xark export` generated and checks the proof
//! with the alt_bn128 syscalls; its public inputs are a `[[u8; 32]; N]`, so their
//! number is checked when the program compiles.
#![cfg_attr(not(test), no_std)]

use over_9000_xark_verifier::{Proof, verify};
use pinocchio::{AccountView, Address, ProgramResult, error::ProgramError, program_entrypoint};

program_entrypoint!(process_instruction);
#[cfg(not(test))]
pinocchio::nostd_panic_handler!();
#[cfg(not(test))]
pinocchio::no_allocator!();

fn process_instruction(
    _program_id: &Address,
    _accounts: &mut [AccountView],
    instruction_data: &[u8],
) -> ProgramResult {
    Verify::try_from(instruction_data)?.process()
}

pub struct VerifyInstructionData<'a> {
    pub proof: &'a [u8; 256],
}

impl<'a> TryFrom<&'a [u8]> for VerifyInstructionData<'a> {
    type Error = ProgramError;

    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        // The proof and nothing else: a byte more or less is not this instruction.
        let proof = data
            .try_into()
            .map_err(|_| ProgramError::InvalidInstructionData)?;
        Ok(Self { proof })
    }
}

pub struct Verify<'a> {
    pub data: VerifyInstructionData<'a>,
}

impl<'a> TryFrom<&'a [u8]> for Verify<'a> {
    type Error = ProgramError;

    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        Ok(Self {
            data: VerifyInstructionData::try_from(data)?,
        })
    }
}

impl Verify<'_> {
    pub fn process(&self) -> ProgramResult {
        if !verify(&Proof::from_le_bytes(self.data.proof), &[]) {
            return Err(Over9000Error::InvalidProof.into());
        }
        Ok(())
    }
}

/// The program's own errors, as `ProgramError::Custom` codes. A proof that doesn't
/// verify is the only failure with no built-in equivalent.
#[repr(u32)]
pub enum Over9000Error {
    InvalidProof = 0,
}

impl From<Over9000Error> for ProgramError {
    fn from(e: Over9000Error) -> Self {
        ProgramError::Custom(e as u32)
    }
}
