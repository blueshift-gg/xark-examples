//! On-chain verifier for the "over 9000" circuit (Pinocchio, no_std, minimal CU).
//! Instruction data is the 256-byte proof: the circuit has no public inputs.
//! `verify_instruction_data` comes from the crate `xark export` generated and
//! checks the proof via the alt_bn128 syscalls. It measures 73,912 CU here, 1,884
//! fewer than the typed `verify(&Proof::from_le_bytes(..), &[])`.
#![cfg_attr(not(test), no_std)]

use over_9000_xark_verifier::verify_instruction_data;
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
    // The proof and nothing else.
    if instruction_data.len() == 256 && verify_instruction_data(instruction_data) {
        Ok(())
    } else {
        Err(ProgramError::InvalidInstructionData)
    }
}
