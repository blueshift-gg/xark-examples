//! Onchain verifier for the "over 9000" circuit. It has one instruction and takes no
//! accounts: the instruction data is the 256-byte proof, as the circuit has no public
//! inputs. `verify` comes from the crate `xark build` generates
//! (`clients/over-9000/rust`) and checks the proof with the alt_bn128 syscalls; its
//! public inputs are the circuit's, none here, so their number is checked when the
//! program compiles.
#![cfg_attr(not(test), no_std)]

use over_9000_verifier::{Proof, verify};
use pinocchio::{
    AccountView, Address, ProgramResult,
    error::{ProgramError, ToStr},
    program_entrypoint,
};

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
    let result = Verify::try_from(instruction_data).and_then(|verify| verify.process());
    // The program's own errors log their names, which the client tests match.
    if let Err(error @ ProgramError::Custom(_)) = &result {
        log(error.to_str::<Over9000Error>());
    }
    result
}

/// `Program log: <message>`.
fn log(message: &str) {
    #[cfg(any(target_os = "solana", target_arch = "bpf"))]
    // SAFETY: the pointer and length are a valid `&str`'s.
    unsafe {
        pinocchio::syscalls::sol_log_(message.as_ptr(), message.len() as u64)
    };
    #[cfg(not(any(target_os = "solana", target_arch = "bpf")))]
    let _ = message;
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
    // Out of line: inlined into the entrypoint under fat LTO, the pairing check's
    // buffers took its frame past the SBF limit of 4 KiB, which only warns at build
    // time and corrupts the stack at run time.
    #[inline(never)]
    pub fn process(&self) -> ProgramResult {
        if !verify(&Proof::from_le_bytes(self.data.proof), []) {
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

impl TryFrom<u32> for Over9000Error {
    type Error = ProgramError;

    fn try_from(code: u32) -> Result<Self, Self::Error> {
        match code {
            0 => Ok(Self::InvalidProof),
            _ => Err(ProgramError::InvalidArgument),
        }
    }
}

impl ToStr for Over9000Error {
    fn to_str(&self) -> &'static str {
        match self {
            Self::InvalidProof => "InvalidProof: invalid proof",
        }
    }
}
