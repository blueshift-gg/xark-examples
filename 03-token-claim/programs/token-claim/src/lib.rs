//! Onchain half of the token claim (Pinocchio). A distributor publishes the Merkle
//! root of a list of `(address, amount)` claims and funds a vault of an SPL token;
//! each address on the list claims its amount once. The program never hashes and
//! never sees a Merkle path: the claimant proves the path in the circuit, and the
//! program checks one proof against public inputs it fills in itself, by name:
//! `root` from the distributor's account, `claimant` from the transaction's
//! signer, `amount` from the instruction. A proof
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

// ETPfSrNBJ6Z29DQ7HzcqHtLcZLWCvS5Ar8VdnoByLZpX
pub const ID: Address = Address::new_from_array([
    0xc7, 0xea, 0xa7, 0xf3, 0x11, 0xc2, 0xe6, 0x66, 0x7a, 0x67, 0xbe, 0x28, 0x04, 0xd3, 0xb2, 0xe5,
    0x7a, 0x54, 0xbc, 0x59, 0x8b, 0x15, 0x52, 0x82, 0x74, 0xad, 0x3e, 0x71, 0x3a, 0xcc, 0xfb, 0xfc,
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
        helpers::log(error.to_str::<TokenClaimError>());
    }
    result
}

fn dispatch(accounts: &mut [AccountView], instruction_data: &[u8]) -> ProgramResult {
    match instruction_data.split_first() {
        Some((Initialize::DISCRIMINATOR, data)) => {
            Initialize::try_from((data, accounts))?.process()
        }
        Some((Claim::DISCRIMINATOR, data)) => Claim::try_from((data, &*accounts))?.process(),
        _ => Err(ProgramError::InvalidInstructionData),
    }
}
