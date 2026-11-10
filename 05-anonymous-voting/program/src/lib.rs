//! Onchain half of anonymous voting (Pinocchio). An authority creates a poll: the
//! Merkle root of its voter list, its option count and a tally per option. A voter
//! proves they're on the list and reveals the poll's nullifier; the program counts
//! the vote and creates a receipt for the nullifier, so a second vote fails. It never
//! learns who voted: the proof says only that someone on the list did, and whoever
//! signs the transaction (a relayer, a fresh wallet) isn't part of it.
//!
//! The program checks each proof against public inputs it assembles itself, in the
//! circuit's order: `root`, `poll` and `options` from the poll's account, `nullifier`
//! and `choice` from the instruction. A proof counts only in its poll, only for its
//! list, and only for its choice.
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

// HrQMmAAVGe9MhJMXMjQYsNA3g9qztY3TMjG9d4gKT1r6
pub const ID: Address = Address::new_from_array([
    0xfa, 0x62, 0xdc, 0x84, 0x03, 0x2e, 0xac, 0x81, 0x44, 0xc9, 0x97, 0x0d, 0xe4, 0x8e, 0x56, 0xdd,
    0x51, 0xf4, 0xd4, 0x42, 0x16, 0xa2, 0x91, 0x94, 0x8e, 0x62, 0xd9, 0xa9, 0x8e, 0x42, 0x8c, 0x6f,
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
        helpers::log(error.to_str::<VotingError>());
    }
    result
}

fn dispatch(accounts: &mut [AccountView], instruction_data: &[u8]) -> ProgramResult {
    match instruction_data.split_first() {
        Some((CreatePoll::DISCRIMINATOR, data)) => {
            CreatePoll::try_from((data, accounts))?.process()
        }
        Some((Vote::DISCRIMINATOR, data)) => Vote::try_from((data, accounts))?.process(),
        Some((ClosePoll::DISCRIMINATOR, data)) => ClosePoll::try_from((data, accounts))?.process(),
        _ => Err(ProgramError::InvalidInstructionData),
    }
}
