//! On-chain verifier for the "over 9000" circuit (Pinocchio, no_std, minimal CU).
//! Instruction data is the 256-byte proof (zero public inputs).
//! `verify_instruction_data` comes from the crate `xark export` generated and
//! checks the proof via the alt_bn128 syscalls.
#![cfg_attr(not(test), no_std)]

use over_9000_xark_verifier as verifier;
use pinocchio::{AccountView, Address, ProgramResult, error::ProgramError, program_entrypoint};

const PROOF_LEN: usize = 256; // zero public inputs → data is just the proof

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
    if instruction_data.len() != PROOF_LEN {
        return Err(ProgramError::InvalidInstructionData);
    }
    if verifier::verify_instruction_data(instruction_data) {
        Ok(())
    } else {
        Err(ProgramError::InvalidInstructionData)
    }
}
