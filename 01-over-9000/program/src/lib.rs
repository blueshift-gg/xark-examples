//! On-chain verifier for the "over 9000" circuit (Pinocchio, no_std, minimal CU).
//! Instruction data is the 256-byte proof: the circuit has no public inputs.
//! `verify` comes from the crate `xark export` generated and checks the proof via
//! the alt_bn128 syscalls; its public inputs are a `[[u8; 32]; N]`, so their number
//! is checked when the program compiles.
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
    // The proof and nothing else.
    let proof: &[u8; 256] = instruction_data
        .try_into()
        .map_err(|_| ProgramError::InvalidInstructionData)?;
    if !verify(&Proof::from_le_bytes(proof), &[]) {
        return Err(ProgramError::InvalidInstructionData);
    }
    Ok(())
}
