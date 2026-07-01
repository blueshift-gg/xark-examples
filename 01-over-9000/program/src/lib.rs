//! On-chain verifier for the "over 9000" circuit.
//!
//! Instruction data is exactly the 256-byte Groth16 proof: this circuit has
//! ZERO public inputs (the `> 9000` threshold is fixed inside the verifying
//! key). If the proof verifies, the sender has proven they know a secret value
//! greater than 9000 — without revealing it.
//!
//! The `verify_instruction_data` function comes from the crate `xark export`
//! generated; it embeds the verifying key at compile time. When the circuit
//! changes, re-run `xark export` — this file never changes.
#![cfg_attr(any(target_os = "solana", target_arch = "bpf"), no_std)]

use over_9000_xark_verifier as verifier;
use pinocchio::{account::AccountView, address::Address, entrypoint, ProgramResult};
use solana_program_error::ProgramError;

entrypoint!(process_instruction);

fn process_instruction(
    _program_id: &Address,
    _accounts: &mut [AccountView],
    instruction_data: &[u8],
) -> ProgramResult {
    // instruction_data == proof (256 B). No public inputs to parse.
    if verifier::verify_instruction_data(instruction_data) {
        Ok(())
    } else {
        Err(ProgramError::InvalidInstructionData)
    }
}
