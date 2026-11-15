use pinocchio::{
    AccountView, Address, ProgramResult,
    cpi::{Seed, Signer},
    error::ProgramError,
};
use pinocchio_token::instructions::Transfer;
use token_claim_verifier::{Proof, PublicInputs, verify};

use crate::{
    TokenClaimError,
    helpers::{create, programs},
    state::{CLAIMED_SEED, DISTRIBUTOR_SEED, Distributor, VAULT_SEED},
};

/// Accounts: the claimant (signer, payer), the distributor, its vault, the receipt,
/// the claimant's token account (the destination), the token program, the system
/// program.
pub struct ClaimAccounts<'a> {
    pub claimant: &'a AccountView,
    pub distributor: &'a AccountView,
    pub vault: &'a AccountView,
    pub receipt: &'a AccountView,
    pub claimant_token: &'a AccountView,
}

impl<'a> TryFrom<&'a [AccountView]> for ClaimAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountView]) -> Result<Self, Self::Error> {
        let [
            claimant,
            distributor,
            vault,
            receipt,
            claimant_token,
            token_program,
            system_program,
            ..,
        ] = accounts
        else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };
        if !claimant.is_signer() {
            return Err(ProgramError::MissingRequiredSignature);
        }
        programs(token_program, system_program)?;
        let vault_bump = Distributor::load(distributor)?.vault_bump;
        let vault_address = Address::derive_address(
            &[VAULT_SEED, distributor.address().as_ref()],
            Some(vault_bump),
            &crate::ID,
        );
        if vault.address() != &vault_address {
            return Err(ProgramError::InvalidSeeds);
        }
        Ok(Self {
            claimant,
            distributor,
            vault,
            receipt,
            claimant_token,
        })
    }
}

/// `proof` (256 B) ‖ `amount` (8 B, little-endian). The proof stays in the
/// instruction data, borrowed: a copy on the stack takes the inlined entrypoint
/// past SBF's 4 KiB frame, and `cargo build-sbf` only warns.
pub struct ClaimInstructionData<'a> {
    pub proof: &'a [u8; 256],
    pub amount: u64,
}

impl<'a> TryFrom<&'a [u8]> for ClaimInstructionData<'a> {
    type Error = ProgramError;

    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        let (proof, amount) = data
            .split_first_chunk::<256>()
            .ok_or(ProgramError::InvalidInstructionData)?;
        let amount: [u8; 8] = amount
            .try_into()
            .map_err(|_| ProgramError::InvalidInstructionData)?;
        Ok(Self {
            proof,
            amount: u64::from_le_bytes(amount),
        })
    }
}

/// Pay `amount` to the claimant's token account, if the proof says the signer is on
/// the list for `amount`. The receipt is created here, so a second claim by the same
/// address fails.
pub struct Claim<'a> {
    pub accounts: ClaimAccounts<'a>,
    pub data: ClaimInstructionData<'a>,
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountView])> for Claim<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a [AccountView])) -> Result<Self, Self::Error> {
        let accounts = ClaimAccounts::try_from(accounts)?;
        let data = ClaimInstructionData::try_from(data)?;
        Ok(Self { accounts, data })
    }
}

impl<'a> Claim<'a> {
    pub const DISCRIMINATOR: &'a u8 = &1;

    pub fn process(&mut self) -> ProgramResult {
        let Self { accounts, data } = self;
        let (root, bump) = {
            let distributor = Distributor::load(accounts.distributor)?;
            (distributor.root, distributor.bump)
        };

        // The circuit's public inputs, each from where the program trusts it: the
        // root from the distributor, the claimant from the signer, the amount from
        // the instruction.
        let inputs = PublicInputs {
            root,
            claim: token_claim_verifier::Claim {
                claimant: *accounts.claimant.address().as_array(),
                amount: data.amount,
            },
        };
        if !verify(&Proof::from_le_bytes(data.proof), inputs) {
            return Err(TokenClaimError::InvalidProof.into());
        }

        // The receipt: once it exists, this address has claimed. It must sit at the
        // canonical bump, or a claimant could claim again at another one.
        let (address, receipt_bump) = Address::find_program_address(
            &[
                CLAIMED_SEED,
                accounts.distributor.address().as_ref(),
                accounts.claimant.address().as_ref(),
            ],
            &crate::ID,
        );
        if accounts.receipt.address() != &address {
            return Err(ProgramError::InvalidSeeds);
        }
        if accounts.receipt.owned_by(&crate::ID) {
            return Err(ProgramError::AccountAlreadyInitialized);
        }
        let receipt_bump = [receipt_bump];
        let receipt_seeds = [
            Seed::from(CLAIMED_SEED),
            Seed::from(accounts.distributor.address().as_ref()),
            Seed::from(accounts.claimant.address().as_ref()),
            Seed::from(&receipt_bump),
        ];
        create(
            accounts.claimant,
            accounts.receipt,
            0,
            &crate::ID,
            &receipt_seeds,
        )?;

        let bump = [bump];
        let seeds = [
            Seed::from(DISTRIBUTOR_SEED),
            Seed::from(&root),
            Seed::from(&bump),
        ];
        Transfer::new(
            accounts.vault,
            accounts.claimant_token,
            accounts.distributor,
            data.amount,
        )
        .invoke_signed(&[Signer::from(&seeds)])
    }
}
