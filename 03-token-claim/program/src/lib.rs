//! Onchain half of the token claim (Pinocchio). A distributor publishes the Merkle
//! root of a list of `(address, amount)` claims and funds a vault of an SPL token;
//! each address on the list claims its amount once. The program never hashes and
//! never sees a Merkle path: the claimant proves the path in the circuit, and the
//! program checks one proof against public inputs it assembles itself, in the
//! circuit's order: `root` from the distributor's account, `claimant` (as two
//! halves) from the transaction's signer, `amount` from the instruction. A proof
//! pays only the wallet it was made for, only its amount, and only from its list.
//!
//! Instruction data starts with one byte, which instruction:
//!
//! - `0`, initialize: `root` (32 B) ‖ `total` (8 B, little-endian). Accounts:
//!   authority (signer, payer), distributor, vault, mint, the authority's token
//!   account (the source of `total`), the token program, the system program.
//! - `1`, claim: proof (256 B) ‖ `amount` (8 B, little-endian). Accounts: claimant
//!   (signer, payer), distributor, vault, receipt, the claimant's token account (the
//!   destination), the token program, the system program.
//!
//! Every account the program owns is a PDA: the distributor
//! `["distributor", root]`, its vault `["vault", distributor]` (a token account the
//! distributor owns, so only the program moves its tokens) and one receipt per claim,
//! `["claimed", distributor, claimant]`, which makes a second claim fail.
#![cfg_attr(not(test), no_std)]

use pinocchio::{
    AccountView, Address, ProgramResult,
    cpi::{Seed, Signer},
    error::ProgramError,
    program_entrypoint,
    sysvars::{Sysvar, rent::Rent},
};
use pinocchio_system::instructions::{Assign, CreateAccount, Transfer as Lamports};
use pinocchio_token::instructions::{InitializeAccount3, Transfer};
use token_claim_xark_verifier::{Proof, verify};

program_entrypoint!(process_instruction);
#[cfg(not(test))]
pinocchio::nostd_panic_handler!();
#[cfg(not(test))]
pinocchio::no_allocator!();

const DISTRIBUTOR_SEED: &[u8] = b"distributor";
const VAULT_SEED: &[u8] = b"vault";
const CLAIMED_SEED: &[u8] = b"claimed";

/// The distributor's state: `root` (32 B) ‖ `mint` (32 B) ‖ its bump ‖ the vault's.
const DISTRIBUTOR_LEN: usize = 66;

/// An SPL token account's size.
const TOKEN_ACCOUNT_LEN: usize = 165;

fn process_instruction(
    program_id: &Address,
    accounts: &mut [AccountView],
    instruction_data: &[u8],
) -> ProgramResult {
    match instruction_data.split_first() {
        Some((0, data)) => initialize(program_id, accounts, data),
        Some((1, data)) => claim(program_id, accounts, data),
        _ => Err(ProgramError::InvalidInstructionData),
    }
}

/// Create the distributor for the list rooted at `root`, and its vault of `mint`,
/// funded with `total` from the authority's token account. Anyone can top the vault
/// up later with a plain SPL transfer.
fn initialize(program_id: &Address, accounts: &mut [AccountView], data: &[u8]) -> ProgramResult {
    let (root, total) = split::<32>(data)?;
    let [authority, distributor, vault, mint, authority_token, ..] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    if !authority.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }

    let (address, bump) = Address::find_program_address(&[DISTRIBUTOR_SEED, root], program_id);
    if distributor.address() != &address {
        return Err(ProgramError::InvalidSeeds);
    }
    let bump = [bump];
    let seeds = [
        Seed::from(DISTRIBUTOR_SEED),
        Seed::from(root),
        Seed::from(&bump),
    ];
    create(authority, distributor, DISTRIBUTOR_LEN, program_id, &seeds)?;

    let (address, vault_bump) =
        Address::find_program_address(&[VAULT_SEED, distributor.address().as_ref()], program_id);
    if vault.address() != &address {
        return Err(ProgramError::InvalidSeeds);
    }
    let vault_bump = [vault_bump];
    let vault_seeds = [
        Seed::from(VAULT_SEED),
        Seed::from(distributor.address().as_ref()),
        Seed::from(&vault_bump),
    ];
    create(
        authority,
        vault,
        TOKEN_ACCOUNT_LEN,
        &pinocchio_token::ID,
        &vault_seeds,
    )?;
    InitializeAccount3::new(vault, mint, distributor.address()).invoke()?;

    let mut state = distributor.try_borrow_mut()?;
    state[..32].copy_from_slice(root);
    state[32..64].copy_from_slice(mint.address().as_ref());
    state[64] = bump[0];
    state[65] = vault_bump[0];
    drop(state);

    Transfer::new(authority_token, vault, authority, total).invoke()
}

/// Pay `amount` to the claimant's token account, if the proof says the signer is on
/// the list for `amount`. The receipt is created here, so a second claim by the same
/// address fails.
fn claim(program_id: &Address, accounts: &mut [AccountView], data: &[u8]) -> ProgramResult {
    let (proof, amount) = split::<256>(data)?;
    let [claimant, distributor, vault, receipt, claimant_token, ..] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    if !claimant.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }
    // Only `initialize` makes an account of the program's this size.
    if !distributor.owned_by(program_id) || distributor.data_len() != DISTRIBUTOR_LEN {
        return Err(ProgramError::InvalidAccountData);
    }
    let (root, bump, vault_bump) = {
        let state = distributor.try_borrow()?;
        let root: [u8; 32] = state[..32].try_into().unwrap();
        (root, state[64], state[65])
    };
    let vault_address = Address::derive_address(
        &[VAULT_SEED, distributor.address().as_ref()],
        Some(vault_bump),
        program_id,
    );
    if vault.address() != &vault_address {
        return Err(ProgramError::InvalidSeeds);
    }

    let public_inputs = public_inputs(&root, claimant.address().as_array(), amount);
    if !verify(&Proof::from_le_bytes(proof), &public_inputs) {
        return Err(ProgramError::InvalidInstructionData);
    }

    // The receipt: once it exists, this address has claimed.
    let (address, receipt_bump) = Address::find_program_address(
        &[
            CLAIMED_SEED,
            distributor.address().as_ref(),
            claimant.address().as_ref(),
        ],
        program_id,
    );
    if receipt.address() != &address {
        return Err(ProgramError::InvalidSeeds);
    }
    if receipt.owned_by(program_id) {
        return Err(ProgramError::AccountAlreadyInitialized);
    }
    let receipt_bump = [receipt_bump];
    let receipt_seeds = [
        Seed::from(CLAIMED_SEED),
        Seed::from(distributor.address().as_ref()),
        Seed::from(claimant.address().as_ref()),
        Seed::from(&receipt_bump),
    ];
    create(claimant, receipt, 0, program_id, &receipt_seeds)?;

    let bump = [bump];
    let seeds = [
        Seed::from(DISTRIBUTOR_SEED),
        Seed::from(&root),
        Seed::from(&bump),
    ];
    Transfer::new(vault, claimant_token, distributor, amount).invoke_signed(&[Signer::from(&seeds)])
}

/// The circuit's public inputs, in its order (`xark inspect --json`): `root`, then
/// `claim`, its `claimant` as `[hi, lo]` and its `amount`. Each a 32-byte
/// little-endian field element.
fn public_inputs(root: &[u8; 32], claimant: &[u8; 32], amount: u64) -> [[u8; 32]; 4] {
    let [hi, lo] = halves(claimant);
    [*root, hi, lo, field(amount)]
}

/// A `u64` as a 32-byte little-endian field element.
fn field(x: u64) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[..8].copy_from_slice(&x.to_le_bytes());
    out
}

/// An address as the circuit's `Hash`: `[hi, lo]`, each 16-byte half read as a
/// big-endian integer, as 32-byte little-endian field elements.
fn halves(bytes: &[u8; 32]) -> [[u8; 32]; 2] {
    let half = |b: &[u8]| {
        let mut out = [0u8; 32];
        out[..16].copy_from_slice(b);
        out[..16].reverse();
        out
    };
    [half(&bytes[..16]), half(&bytes[16..])]
}

/// `N` bytes, then a little-endian `u64`, and nothing else.
fn split<const N: usize>(data: &[u8]) -> Result<(&[u8; N], u64), ProgramError> {
    let (head, tail) = data
        .split_first_chunk::<N>()
        .ok_or(ProgramError::InvalidInstructionData)?;
    let tail: [u8; 8] = tail
        .try_into()
        .map_err(|_| ProgramError::InvalidInstructionData)?;
    Ok((head, u64::from_le_bytes(tail)))
}

/// Create the PDA `account` (signed by `seeds`), `space` bytes owned by `owner`, paid
/// by `payer`. An account anyone can send lamports to could otherwise be blocked
/// that way: `CreateAccount` refuses an address that holds any. So an account that
/// does is topped up to rent-exempt and assigned instead (a receipt or a distributor
/// that's still the system program's has no data, and only its seeds can sign).
fn create(
    payer: &AccountView,
    account: &AccountView,
    space: usize,
    owner: &Address,
    seeds: &[Seed],
) -> ProgramResult {
    let signer = [Signer::from(seeds)];
    let rent = Rent::get()?.try_minimum_balance(space)?;
    let held = account.lamports();
    if held == 0 {
        return CreateAccount {
            from: payer,
            to: account,
            lamports: rent,
            space: space as u64,
            owner,
        }
        .invoke_signed(&signer);
    }
    if held < rent {
        Lamports {
            from: payer,
            to: account,
            lamports: rent - held,
        }
        .invoke()?;
    }
    if space > 0 {
        pinocchio_system::instructions::Allocate {
            account,
            space: space as u64,
        }
        .invoke_signed(&signer)?;
    }
    Assign { account, owner }.invoke_signed(&signer)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The demo proof (`examples/demo.rs`: a list of one claim, 1000 for `[7; 32]`)
    /// verifies against the public inputs the program assembles, for its claimant,
    /// amount and root only.
    #[test]
    fn the_demo_proof_verifies_for_its_claim_only() {
        let proof = include_bytes!("../../circuit/token_claim-verifier/proof.solana.bin");
        let inputs = include_bytes!("../../circuit/token_claim-verifier/public_inputs.solana.bin");
        let proof = Proof::from_le_bytes(proof);
        let root: &[u8; 32] = inputs.first_chunk().unwrap();
        assert!(verify(&proof, &public_inputs(root, &[7; 32], 1_000)));
        assert!(!verify(&proof, &public_inputs(root, &[8; 32], 1_000)));
        assert!(!verify(&proof, &public_inputs(root, &[7; 32], 1_001)));
        assert!(!verify(&proof, &public_inputs(&[1; 32], &[7; 32], 1_000)));
    }
}
