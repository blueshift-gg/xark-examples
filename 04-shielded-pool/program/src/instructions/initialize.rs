use pinocchio::{
    AccountView, Address, ProgramResult,
    cpi::{Seed, Signer},
    error::ProgramError,
};

use crate::{
    helpers::{is_uninitialized, system_program, writable, writable_signer},
    state::{POOL_SEED, Pool},
};

pub struct InitializeAccounts<'a> {
    pub pool: &'a mut AccountView,
    pub authority: &'a AccountView,
}

impl<'a> TryFrom<&'a mut [AccountView]> for InitializeAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a mut [AccountView]) -> Result<Self, Self::Error> {
        let [pool, authority, system, ..] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };
        writable_signer(authority)?;
        system_program(system)?;
        writable(pool)?;
        Ok(Self { pool, authority })
    }
}

/// `denomination (u64 LE)`.
pub struct InitializeInstructionData {
    pub denomination: [u8; 8],
}

impl TryFrom<&[u8]> for InitializeInstructionData {
    type Error = ProgramError;

    fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
        let denomination = data
            .try_into()
            .map_err(|_| ProgramError::InvalidInstructionData)?;
        Ok(Self { denomination })
    }
}

/// Create the pool for one denomination. Permissionless: the first caller fixes
/// the size (fine for a demo); the empty-tree root is a program constant, so the
/// pool cannot be seeded with a wrong root.
pub struct Initialize<'a> {
    pub accounts: InitializeAccounts<'a>,
    pub data: InitializeInstructionData,
    pub bump: u8,
}

impl<'a> TryFrom<(&'a [u8], &'a mut [AccountView])> for Initialize<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a mut [AccountView])) -> Result<Self, Self::Error> {
        let accounts = InitializeAccounts::try_from(accounts)?;
        let data = InitializeInstructionData::try_from(data)?;

        // One pool per denomination (like Tornado's separate per-size pools), at the
        // canonical bump, which the pool stores for every later check.
        let (address, bump) =
            Address::find_program_address(&[POOL_SEED, &data.denomination], &crate::ID);
        if accounts.pool.address() != &address {
            return Err(ProgramError::InvalidSeeds);
        }
        if !is_uninitialized(accounts.pool) {
            return Err(ProgramError::AccountAlreadyInitialized);
        }
        Ok(Self {
            accounts,
            data,
            bump,
        })
    }
}

impl<'a> Initialize<'a> {
    pub const DISCRIMINATOR: &'a u8 = &0;

    pub fn process(&mut self) -> ProgramResult {
        let bump = [self.bump];
        let seeds = [
            Seed::from(POOL_SEED),
            Seed::from(&self.data.denomination),
            Seed::from(&bump),
        ];
        pinocchio_system::create_account_with_minimum_balance_signed(
            self.accounts.pool,
            Pool::LEN,
            &crate::ID,
            self.accounts.authority,
            None,
            &[Signer::from(&seeds)],
        )?;

        let mut data = self.accounts.pool.try_borrow_mut()?;
        Pool::init(&mut data, self.data.denomination, self.bump)
    }
}
