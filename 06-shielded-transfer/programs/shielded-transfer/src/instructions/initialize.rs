use pinocchio::{
    AccountView, Address, ProgramResult,
    cpi::{Seed, Signer},
    error::ProgramError,
};
use pinocchio_system::create_account_with_minimum_balance_signed;
use pinocchio_token::{
    instructions::InitializeAccount3,
    state::{Account as TokenAccount, Mint},
};

use crate::{
    helpers::{programs, signer, writable},
    state::{EMPTY_ROOT, POOL_SEED, Pool, ROOT_HISTORY_SIZE, VAULT_SEED},
};

/// The pool and its vault, for an initialized SPL mint, both not yet created.
pub struct InitializeAccounts<'a> {
    pub pool: &'a mut AccountView,
    pub vault: &'a mut AccountView,
    pub mint: &'a AccountView,
    pub authority: &'a AccountView,
    pub pool_bump: u8,
    pub vault_bump: u8,
}

impl<'a> TryFrom<&'a mut [AccountView]> for InitializeAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a mut [AccountView]) -> Result<Self, Self::Error> {
        let [
            pool,
            vault,
            mint,
            authority,
            token_program,
            system_program,
            ..,
        ] = accounts
        else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };
        signer(authority)?;
        programs(token_program, system_program)?;
        if !Mint::from_account_view(mint)?.is_initialized() {
            return Err(ProgramError::UninitializedAccount);
        }
        // One pool per mint: each PDA at its canonical bump, which the pool stores
        // for every later check.
        let mint_key = mint.address().as_ref();
        let pool_bump = fresh_pda(pool, &[POOL_SEED, mint_key])?;
        let vault_bump = fresh_pda(vault, &[VAULT_SEED, mint_key])?;
        Ok(Self {
            pool,
            vault,
            mint,
            authority,
            pool_bump,
            vault_bump,
        })
    }
}

/// `account` is writable, at the canonical PDA of `seeds`, and not yet created (the
/// system program's and empty; it may hold lamports, which creation tops up, so
/// funding the address first can't block it). Returns the bump.
fn fresh_pda(account: &AccountView, seeds: &[&[u8]]) -> Result<u8, ProgramError> {
    writable(account)?;
    let (address, bump) = Address::find_program_address(seeds, &crate::ID);
    if account.address() != &address {
        return Err(ProgramError::InvalidSeeds);
    }
    if !account.owned_by(&pinocchio_system::ID) || !account.is_data_empty() {
        return Err(ProgramError::AccountAlreadyInitialized);
    }
    Ok(bump)
}

/// Create the pool and its token vault for an SPL mint. No arguments.
pub struct Initialize<'a> {
    pub accounts: InitializeAccounts<'a>,
}

impl<'a> TryFrom<(&'a [u8], &'a mut [AccountView])> for Initialize<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a mut [AccountView])) -> Result<Self, Self::Error> {
        if !data.is_empty() {
            return Err(ProgramError::InvalidInstructionData);
        }
        let accounts = InitializeAccounts::try_from(accounts)?;
        Ok(Self { accounts })
    }
}

impl<'a> Initialize<'a> {
    pub const DISCRIMINATOR: &'a u8 = &0;

    pub fn process(&mut self) -> ProgramResult {
        let a = &mut self.accounts;
        let mint_account = a.mint;
        let mint = mint_account.address().as_array();

        let pool_bump = [a.pool_bump];
        let seeds = [
            Seed::from(POOL_SEED),
            Seed::from(mint),
            Seed::from(&pool_bump),
        ];
        create_account_with_minimum_balance_signed(
            a.pool,
            Pool::LEN,
            &crate::ID,
            a.authority,
            None,
            &[Signer::from(&seeds)],
        )?;
        {
            let mut state = Pool::from_account_view_mut(a.pool)?;
            state.mint = *mint;
            state.next_index = 0u32.to_le_bytes();
            state.current_root_index = 0u32.to_le_bytes();
            state.roots = [[0; 32]; ROOT_HISTORY_SIZE];
            state.roots[0] = EMPTY_ROOT;
            state.bump = a.pool_bump;
            state.vault_bump = a.vault_bump;
        }

        let vault_bump = [a.vault_bump];
        let seeds = [
            Seed::from(VAULT_SEED),
            Seed::from(mint),
            Seed::from(&vault_bump),
        ];
        create_account_with_minimum_balance_signed(
            a.vault,
            TokenAccount::LEN,
            &pinocchio_token::ID,
            a.authority,
            None,
            &[Signer::from(&seeds)],
        )?;
        InitializeAccount3::new(a.vault, a.mint, a.pool.address()).invoke()
    }
}
