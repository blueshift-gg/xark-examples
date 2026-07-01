//! On-chain verifier for the age-verification circuit.
//!
//! Instruction data: `proof (256 B) || commitment (32 B) || current_year (32 B)`.
//! Public inputs are 32-byte **little-endian** field elements, in the order the
//! circuit declares its `pub` params: `[commitment, current_year]`.
//!
//! This program demonstrates *consuming* a public input: it reads
//! `current_year` and sanity-checks it. A production deployment would go
//! further (see README): compare `current_year` against the on-chain `Clock`,
//! and check `commitment` against a trusted issuer-registry account so that
//! only credentials from a real issuer are accepted.
#![cfg_attr(any(target_os = "solana", target_arch = "bpf"), no_std)]

use age_verification_xark_verifier as verifier;
use pinocchio::{account::AccountView, address::Address, entrypoint, ProgramResult};
use solana_program_error::ProgramError;

const PROOF_LEN: usize = 256;
const FR: usize = 32;
const N_PUBLIC: usize = 2; // commitment, current_year

entrypoint!(process_instruction);

fn process_instruction(
    _program_id: &Address,
    _accounts: &mut [AccountView],
    instruction_data: &[u8],
) -> ProgramResult {
    if instruction_data.len() != PROOF_LEN + N_PUBLIC * FR {
        return Err(ProgramError::InvalidInstructionData);
    }

    // public_inputs[1] = current_year, at offset PROOF_LEN + 1*FR, LE.
    let current_year = read_u64_le(&instruction_data[PROOF_LEN + FR..PROOF_LEN + 2 * FR]);

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
