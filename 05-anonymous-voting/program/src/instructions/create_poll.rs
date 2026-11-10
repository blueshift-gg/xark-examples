use pinocchio::{
    AccountView, Address, ProgramResult,
    cpi::{Seed, Signer},
    error::ProgramError,
};

use crate::{
    helpers::{is_uninitialized, system_program, writable, writable_signer},
    state::{MAX_OPTIONS, POLL_SEED, Poll},
};

pub struct CreatePollAccounts<'a> {
    pub authority: &'a AccountView,
    pub poll: &'a mut AccountView,
}

impl<'a> TryFrom<&'a mut [AccountView]> for CreatePollAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a mut [AccountView]) -> Result<Self, Self::Error> {
        let [authority, poll, system, ..] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };
        writable_signer(authority)?;
        writable(poll)?;
        system_program(system)?;
        Ok(Self { authority, poll })
    }
}

/// `poll_id (u64 LE) ‖ root (32) ‖ options (1)`.
pub struct CreatePollInstructionData<'a> {
    pub poll_id: [u8; 8],
    pub root: &'a [u8; 32],
    pub options: u8,
}

impl<'a> TryFrom<&'a [u8]> for CreatePollInstructionData<'a> {
    type Error = ProgramError;

    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        let (poll_id, data) = data
            .split_first_chunk::<8>()
            .ok_or(ProgramError::InvalidInstructionData)?;
        let (root, data) = data
            .split_first_chunk::<32>()
            .ok_or(ProgramError::InvalidInstructionData)?;
        let [options] = *data else {
            return Err(ProgramError::InvalidInstructionData);
        };
        // At least a real choice, and no more than the tally has room for.
        if !(2..=MAX_OPTIONS as u8).contains(&options) {
            return Err(ProgramError::InvalidArgument);
        }
        Ok(Self {
            poll_id: *poll_id,
            root,
            options,
        })
    }
}

/// Create poll `poll_id` over the voter list rooted at `root`, with `options`
/// options, each tally at zero. The authority signs, pays, and is the only one who
/// can close it.
pub struct CreatePoll<'a> {
    pub accounts: CreatePollAccounts<'a>,
    pub data: CreatePollInstructionData<'a>,
    pub bump: u8,
}

impl<'a> TryFrom<(&'a [u8], &'a mut [AccountView])> for CreatePoll<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a mut [AccountView])) -> Result<Self, Self::Error> {
        let accounts = CreatePollAccounts::try_from(accounts)?;
        let data = CreatePollInstructionData::try_from(data)?;

        // The poll's id is its only seed, so no two polls share one, and no two polls
        // share a voter's nullifier. The canonical bump is stored for later checks.
        let (address, bump) =
            Address::find_program_address(&[POLL_SEED, &data.poll_id], &crate::ID);
        if accounts.poll.address() != &address {
            return Err(ProgramError::InvalidSeeds);
        }
        if !is_uninitialized(accounts.poll) {
            return Err(ProgramError::AccountAlreadyInitialized);
        }
        Ok(Self {
            accounts,
            data,
            bump,
        })
    }
}

impl<'a> CreatePoll<'a> {
    pub const DISCRIMINATOR: &'a u8 = &0;

    pub fn process(&mut self) -> ProgramResult {
        let bump = [self.bump];
        let seeds = [
            Seed::from(POLL_SEED),
            Seed::from(&self.data.poll_id),
            Seed::from(&bump),
        ];
        pinocchio_system::create_account_with_minimum_balance_signed(
            self.accounts.poll,
            Poll::LEN,
            &crate::ID,
            self.accounts.authority,
            None,
            &[Signer::from(&seeds)],
        )?;

        let mut data = self.accounts.poll.try_borrow_mut()?;
        Poll::init(
            &mut data,
            self.data.poll_id,
            self.accounts.authority.address(),
            self.data.root,
            self.data.options,
            self.bump,
        )
    }
}
