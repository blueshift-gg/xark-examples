use pinocchio::{AccountView, ProgramResult, error::ProgramError};

use crate::state::Poll;

pub struct ClosePollAccounts<'a> {
    pub poll: &'a mut AccountView,
}

impl<'a> TryFrom<&'a mut [AccountView]> for ClosePollAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a mut [AccountView]) -> Result<Self, Self::Error> {
        let [authority, poll, ..] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };
        if !authority.is_signer() {
            return Err(ProgramError::MissingRequiredSignature);
        }
        Poll::check(poll)?;
        if Poll::load(&poll.try_borrow()?)?.authority != *authority.address() {
            return Err(ProgramError::IncorrectAuthority);
        }
        Ok(Self { poll })
    }
}

/// Freeze the poll: no vote counts after this, and the tally is final. Only its
/// authority can.
pub struct ClosePoll<'a> {
    pub accounts: ClosePollAccounts<'a>,
}

impl<'a> TryFrom<(&'a [u8], &'a mut [AccountView])> for ClosePoll<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a mut [AccountView])) -> Result<Self, Self::Error> {
        if !data.is_empty() {
            return Err(ProgramError::InvalidInstructionData);
        }
        let accounts = ClosePollAccounts::try_from(accounts)?;
        Ok(Self { accounts })
    }
}

impl<'a> ClosePoll<'a> {
    pub const DISCRIMINATOR: &'a u8 = &2;

    pub fn process(&mut self) -> ProgramResult {
        Poll::load_mut(&mut self.accounts.poll.try_borrow_mut()?)?.close();
        Ok(())
    }
}
