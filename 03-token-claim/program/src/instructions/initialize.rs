use pinocchio::{AccountView, Address, ProgramResult, cpi::Seed, error::ProgramError};
use pinocchio_token::instructions::{InitializeAccount3, Transfer};

use crate::{
    helpers::create,
    state::{DISTRIBUTOR_SEED, Distributor, TOKEN_ACCOUNT_LEN, VAULT_SEED},
};

/// Accounts: the authority (signer, payer), the distributor, its vault, the mint,
/// the authority's token account (the source of `total`), the token program, the
/// system program.
pub struct InitializeAccounts<'a> {
    pub authority: &'a AccountView,
    pub distributor: &'a mut AccountView,
    pub vault: &'a AccountView,
    pub mint: &'a AccountView,
    pub authority_token: &'a AccountView,
}

impl<'a> TryFrom<&'a mut [AccountView]> for InitializeAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a mut [AccountView]) -> Result<Self, Self::Error> {
        let [authority, distributor, vault, mint, authority_token, ..] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };
        if !authority.is_signer() {
            return Err(ProgramError::MissingRequiredSignature);
        }
        Ok(Self {
            authority,
            distributor,
            vault,
            mint,
            authority_token,
        })
    }
}

/// `root` (32 B) ‖ `total` (8 B, little-endian).
pub struct InitializeInstructionData {
    pub root: [u8; 32],
    pub total: u64,
}

impl TryFrom<&[u8]> for InitializeInstructionData {
    type Error = ProgramError;

    fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
        let (root, total) = data
            .split_first_chunk::<32>()
            .ok_or(ProgramError::InvalidInstructionData)?;
        let total: [u8; 8] = total
            .try_into()
            .map_err(|_| ProgramError::InvalidInstructionData)?;
        Ok(Self {
            root: *root,
            total: u64::from_le_bytes(total),
        })
    }
}

/// Create the distributor for the list rooted at `root`, and its vault of `mint`,
/// funded with `total` from the authority's token account. Anyone can top the vault
/// up later with a plain SPL transfer.
pub struct Initialize<'a> {
    pub program_id: &'a Address,
    pub accounts: InitializeAccounts<'a>,
    pub data: InitializeInstructionData,
    pub bump: u8,
    pub vault_bump: u8,
}

impl<'a> TryFrom<(&'a Address, &'a [u8], &'a mut [AccountView])> for Initialize<'a> {
    type Error = ProgramError;

    fn try_from(
        (program_id, data, accounts): (&'a Address, &'a [u8], &'a mut [AccountView]),
    ) -> Result<Self, Self::Error> {
        let accounts = InitializeAccounts::try_from(accounts)?;
        let data = InitializeInstructionData::try_from(data)?;

        // The bumps are found once, here, and stored for every claim to reuse.
        let (address, bump) =
            Address::find_program_address(&[DISTRIBUTOR_SEED, &data.root], program_id);
        if accounts.distributor.address() != &address {
            return Err(ProgramError::InvalidSeeds);
        }
        let (address, vault_bump) = Address::find_program_address(
            &[VAULT_SEED, accounts.distributor.address().as_ref()],
            program_id,
        );
        if accounts.vault.address() != &address {
            return Err(ProgramError::InvalidSeeds);
        }

        Ok(Self {
            program_id,
            accounts,
            data,
            bump,
            vault_bump,
        })
    }
}

impl<'a> Initialize<'a> {
    pub const DISCRIMINATOR: &'a u8 = &0;

    pub fn process(&mut self) -> ProgramResult {
        let Self {
            program_id,
            accounts,
            data,
            bump,
            vault_bump,
        } = self;

        let bump_seed = [*bump];
        let seeds = [
            Seed::from(DISTRIBUTOR_SEED),
            Seed::from(&data.root),
            Seed::from(&bump_seed),
        ];
        create(
            accounts.authority,
            accounts.distributor,
            Distributor::LEN,
            program_id,
            &seeds,
        )?;

        let vault_bump_seed = [*vault_bump];
        let vault_seeds = [
            Seed::from(VAULT_SEED),
            Seed::from(accounts.distributor.address().as_ref()),
            Seed::from(&vault_bump_seed),
        ];
        create(
            accounts.authority,
            accounts.vault,
            TOKEN_ACCOUNT_LEN,
            &pinocchio_token::ID,
            &vault_seeds,
        )?;
        InitializeAccount3::new(
            accounts.vault,
            accounts.mint,
            accounts.distributor.address(),
        )
        .invoke()?;

        let mut distributor = Distributor::load_mut(accounts.distributor, program_id)?;
        distributor.root = data.root;
        distributor.mint = *accounts.mint.address();
        distributor.bump = *bump;
        distributor.vault_bump = *vault_bump;
        drop(distributor);

        Transfer::new(
            accounts.authority_token,
            accounts.vault,
            accounts.authority,
            data.total,
        )
        .invoke()
    }
}
