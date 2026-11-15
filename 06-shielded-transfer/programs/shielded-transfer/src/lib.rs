//! Shielded transfer, the onchain half (Pinocchio + SPL).
//!
//! One `transact` instruction covers deposit, private transfer and withdraw through
//! the JoinSplit value slots. The chain never hashes a note: it verifies the proof,
//! checks the anchor root is remembered, burns the two nullifiers, advances the tree
//! to `new_root`, moves SPL for the public value slots, and logs the two proof-bound
//! encrypted memos for scanning wallets.
//!
//! Reference implementation, unaudited and educational. See the README.
#![cfg_attr(not(test), no_std)]

pub mod errors;
pub mod helpers;
pub mod instructions;
pub mod state;

use errors::ShieldedTransferError;
use instructions::*;
use pinocchio::{AccountView, Address, ProgramResult, error::ProgramError, program_entrypoint};

program_entrypoint!(process_instruction);
#[cfg(not(test))]
pinocchio::nostd_panic_handler!();
#[cfg(not(test))]
pinocchio::no_allocator!();

// CUCcJJBRbK6tK4nPP2zgmvdYbKWSCGGVdRegtJ2GPJtF
pub const ID: Address = Address::new_from_array([
    0xaa, 0x68, 0x5d, 0x29, 0x28, 0x7d, 0x47, 0xbd, 0xdd, 0xfd, 0x73, 0xfa, 0x40, 0xf1, 0xd4, 0xb7,
    0xa5, 0xa4, 0x78, 0x46, 0xc0, 0xd8, 0x4b, 0x91, 0xa8, 0x42, 0x2f, 0x90, 0x8a, 0x31, 0xce, 0xc0,
]);

fn process_instruction(
    _program_id: &Address,
    accounts: &mut [AccountView],
    instruction_data: &[u8],
) -> ProgramResult {
    let result = dispatch(accounts, instruction_data);
    // The program's own errors log their names, which the client tests match.
    if let Err(error @ ProgramError::Custom(_)) = &result {
        helpers::log(error.to_str::<ShieldedTransferError>());
    }
    result
}

fn dispatch(accounts: &mut [AccountView], instruction_data: &[u8]) -> ProgramResult {
    match instruction_data.split_first() {
        Some((Initialize::DISCRIMINATOR, data)) => {
            Initialize::try_from((data, accounts))?.process()
        }
        Some((Transact::DISCRIMINATOR, data)) => Transact::try_from((data, accounts))?.process(),
        _ => Err(ProgramError::InvalidInstructionData),
    }
}

#[cfg(test)]
mod tests;
