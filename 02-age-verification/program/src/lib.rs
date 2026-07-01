//! On-chain verifier for the age-verification circuit (Pinocchio).
//! Instruction data: proof (256 B) || commitment (32 B) || current_year (32 B).
//! Public inputs are 32-byte little-endian, in `pub`-declaration order. Reading
//! current_year here shows public-input parsing; a real deployment would also
//! check the Clock and a trusted issuer registry (see README).
#![cfg_attr(not(test), no_std)]

use age_verification_xark_verifier as verifier;
use pinocchio::{error::ProgramError, program_entrypoint, AccountView, Address, ProgramResult};

const PROOF_LEN: usize = 256;
const FR: usize = 32;
const N_PUBLIC: usize = 2; // commitment, current_year

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
    if instruction_data.len() != PROOF_LEN + N_PUBLIC * FR {
        return Err(ProgramError::InvalidInstructionData);
    }

    // public_inputs[1] = current_year, at offset PROOF_LEN + 1*FR, LE.
    let year_bytes = &instruction_data[PROOF_LEN + FR..PROOF_LEN + 2 * FR];
    // current_year is a u32 in-circuit, so the field's upper bytes must be zero;
    // check that before trusting the low-8 read (don't blindly truncate a field).
    if year_bytes[8..].iter().any(|&b| b != 0) {
        return Err(ProgramError::InvalidInstructionData);
    }
    let current_year = read_u64_le(year_bytes);

    // Illustrative freshness bound. Real programs: assert against Clock sysvar.
    if !(2026..2100).contains(&current_year) {
        return Err(ProgramError::InvalidInstructionData);
    }

    if verifier::verify_instruction_data(instruction_data) {
        Ok(())
    } else {
        Err(ProgramError::InvalidInstructionData)
    }
}

/// Interpret the low 8 bytes of a 32-byte little-endian field element as u64.
fn read_u64_le(bytes: &[u8]) -> u64 {
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&bytes[..8]);
    u64::from_le_bytes(buf)
}
