//! Shielded pool: the onchain half (Pinocchio).
//!
//! The program never hashes a tree node. All Poseidon lives in the circuits; the
//! chain keeps only what consensus needs (a ring of recent Merkle roots, the next
//! leaf index, and one marker account per spent nullifier) and verifies two proofs
//! with the crates `xark export` generated:
//!
//!   deposit   "new_root extends the current tree by one leaf"  → store new_root
//!   withdraw  "I own some leaf, here is its nullifier"          → pay out once
//!
//! Each proof is checked against public inputs the program assembles itself,
//! 32-byte little-endian field elements in the circuit's order, so a proof only
//! holds for the state and accounts of the transaction carrying it.
//!
//! Reference implementation: unaudited, educational. See the README.
#![cfg_attr(not(test), no_std)]

use pinocchio::{AccountView, Address, ProgramResult, error::ProgramError, program_entrypoint};

mod errors;
mod helpers;
mod instructions;
mod state;
#[cfg(test)]
mod tests;

pub use errors::*;
pub use instructions::*;
pub use state::*;

// Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS
pub const ID: Address = Address::new_from_array([
    0xda, 0x07, 0x5c, 0xb2, 0xff, 0x5e, 0xc6, 0x81, 0x76, 0x13, 0xde, 0x53, 0x0b, 0x69, 0x2a, 0x87,
    0x35, 0x47, 0x77, 0x69, 0xda, 0x47, 0x43, 0x0c, 0xbd, 0x81, 0x54, 0x33, 0x5c, 0x4a, 0x83, 0x27,
]);

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
        helpers::log(error.to_str::<ShieldedPoolError>());
    }
    result
}

fn dispatch(accounts: &mut [AccountView], instruction_data: &[u8]) -> ProgramResult {
    match instruction_data.split_first() {
        Some((Initialize::DISCRIMINATOR, data)) => {
            Initialize::try_from((data, accounts))?.process()
        }
        Some((Deposit::DISCRIMINATOR, data)) => Deposit::try_from((data, accounts))?.process(),
        Some((Withdraw::DISCRIMINATOR, data)) => Withdraw::try_from((data, accounts))?.process(),
        _ => Err(ProgramError::InvalidInstructionData),
    }
}
