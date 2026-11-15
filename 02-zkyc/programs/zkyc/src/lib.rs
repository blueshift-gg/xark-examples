//! Onchain verifier for the zkyc circuit (Pinocchio). One instruction, `VerifyKyc`:
//! the holder signs, and the instruction data carries a proof and the commitment it
//! is for. The program supplies every other public input: its own policy, on today's
//! date from the Clock sysvar, and the holder's address. The verifier picks the
//! rules, not the prover: a proof made against a laxer policy, for another day, or
//! for another wallet doesn't verify. A real deployment would also check the
//! commitment against an issuer's registry (see README).
#![cfg_attr(not(test), no_std)]

use pinocchio::{AccountView, Address, ProgramResult, error::ProgramError, program_entrypoint};

mod errors;
mod instructions;
mod policy;
#[cfg(test)]
mod tests;

pub use errors::*;
pub use instructions::*;

program_entrypoint!(process_instruction);
#[cfg(not(test))]
pinocchio::nostd_panic_handler!();
#[cfg(not(test))]
pinocchio::no_allocator!();

fn process_instruction(
    _program_id: &Address,
    accounts: &mut [AccountView],
    instruction_data: &[u8],
) -> ProgramResult {
    let result = dispatch(accounts, instruction_data);
    // The program's own errors log their names, which the client tests match.
    if let Err(error @ ProgramError::Custom(_)) = &result {
        log(error.to_str::<ZkycError>());
    }
    result
}

fn dispatch(accounts: &mut [AccountView], instruction_data: &[u8]) -> ProgramResult {
    match instruction_data.split_first() {
        Some((VerifyKyc::DISCRIMINATOR, data)) => {
            VerifyKyc::try_from((data, &*accounts))?.process()
        }
        _ => Err(ProgramError::InvalidInstructionData),
    }
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
