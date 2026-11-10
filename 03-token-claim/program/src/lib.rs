//! Onchain half of the token claim (Pinocchio). A distributor publishes the Merkle
//! root of a list of `(address, amount)` claims and funds a vault of an SPL token;
//! each address on the list claims its amount once. The program never hashes and
//! never sees a Merkle path: the claimant proves the path in the circuit, and the
//! program checks one proof against public inputs it assembles itself, in the
//! circuit's order: `root` from the distributor's account, `claimant` (as two
//! halves) from the transaction's signer, `amount` from the instruction. A proof
//! pays only the wallet it was made for, only its amount, and only from its list.
//!
//! Instruction data starts with one byte, which instruction: `0` is
//! [`Initialize`], `1` is [`Claim`].
#![cfg_attr(not(test), no_std)]

pub mod errors;
pub mod helpers;
pub mod instructions;
pub mod state;
#[cfg(test)]
mod tests;

pub use errors::*;
pub use instructions::*;

use pinocchio::{AccountView, Address, ProgramResult, error::ProgramError, program_entrypoint};

program_entrypoint!(process_instruction);
#[cfg(not(test))]
pinocchio::nostd_panic_handler!();
#[cfg(not(test))]
pinocchio::no_allocator!();

fn process_instruction(
    program_id: &Address,
    accounts: &mut [AccountView],
    instruction_data: &[u8],
) -> ProgramResult {
    // The error's name goes in the logs, so a client sees `InvalidProof` rather
    // than a bare custom code.
    dispatch(program_id, accounts, instruction_data)
        .inspect_err(|error| helpers::log(error.to_str::<TokenClaimError>()))
}

fn dispatch(
    program_id: &Address,
    accounts: &mut [AccountView],
    instruction_data: &[u8],
) -> ProgramResult {
    match instruction_data.split_first() {
        Some((Initialize::DISCRIMINATOR, data)) => {
            Initialize::try_from((program_id, data, accounts))?.process()
        }
        Some((Claim::DISCRIMINATOR, data)) => {
            Claim::try_from((program_id, data, &*accounts))?.process()
        }
        _ => Err(ProgramError::InvalidInstructionData),
    }
}
