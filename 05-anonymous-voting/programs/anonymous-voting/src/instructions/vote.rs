use pinocchio::{
    AccountView, Address, ProgramResult,
    cpi::{Seed, Signer},
    error::ProgramError,
};
use vote_verifier::{Ballot, Proof, PublicInputs, fields, verify};

use crate::{
    VotingError,
    helpers::{is_uninitialized, system_program, writable, writable_signer},
    state::{Poll, VOTED_SEED},
};

pub struct VoteAccounts<'a> {
    pub payer: &'a AccountView,
    pub poll: &'a mut AccountView,
    pub receipt: &'a mut AccountView,
}

impl<'a> TryFrom<&'a mut [AccountView]> for VoteAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a mut [AccountView]) -> Result<Self, Self::Error> {
        let [payer, poll, receipt, system, ..] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };
        // Anyone pays: the proof doesn't name the payer, so a relayer or a fresh
        // wallet can submit it without the voter's wallet appearing.
        writable_signer(payer)?;
        Poll::check(poll)?;
        writable(receipt)?;
        system_program(system)?;
        Ok(Self {
            payer,
            poll,
            receipt,
        })
    }
}

/// `proof (256) ‖ nullifier (32) ‖ choice (1)`.
pub struct VoteInstructionData<'a> {
    pub proof: &'a [u8; 256],
    pub nullifier: &'a [u8; 32],
    pub choice: u8,
}

impl<'a> TryFrom<&'a [u8]> for VoteInstructionData<'a> {
    type Error = ProgramError;

    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        let (proof, data) = data
            .split_first_chunk::<256>()
            .ok_or(ProgramError::InvalidInstructionData)?;
        let (nullifier, data) = data
            .split_first_chunk::<32>()
            .ok_or(ProgramError::InvalidInstructionData)?;
        let [choice] = *data else {
            return Err(ProgramError::InvalidInstructionData);
        };
        Ok(Self {
            proof,
            nullifier,
            choice,
        })
    }
}

/// Count one vote for `choice` in the poll, if the proof says a voter on its list
/// made it and `nullifier` is theirs in this poll. The receipt for the nullifier is
/// created here, so the same voter can't vote in this poll again.
pub struct Vote<'a> {
    pub accounts: VoteAccounts<'a>,
    pub data: VoteInstructionData<'a>,
    pub receipt_bump: u8,
}

impl<'a> TryFrom<(&'a [u8], &'a mut [AccountView])> for Vote<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a mut [AccountView])) -> Result<Self, Self::Error> {
        let accounts = VoteAccounts::try_from(accounts)?;
        let data = VoteInstructionData::try_from(data)?;

        // The receipt must sit at the canonical bump: another bump would be another
        // address for the same nullifier, and the voter could vote once at each.
        let seeds = [VOTED_SEED, accounts.poll.address().as_ref(), data.nullifier];
        let (address, receipt_bump) = Address::find_program_address(&seeds, &crate::ID);
        if accounts.receipt.address() != &address {
            return Err(ProgramError::InvalidSeeds);
        }
        if !is_uninitialized(accounts.receipt) {
            return Err(VotingError::AlreadyVoted.into());
        }
        Ok(Self {
            accounts,
            data,
            receipt_bump,
        })
    }
}

impl<'a> Vote<'a> {
    pub const DISCRIMINATOR: &'a u8 = &1;

    pub fn process(&mut self) -> ProgramResult {
        let inputs = {
            let data = self.accounts.poll.try_borrow()?;
            let poll = Poll::load(&data)?;
            if !poll.is_open() {
                return Err(VotingError::PollClosed.into());
            }
            public_inputs(poll, self.data.nullifier, self.data.choice)
        };
        // The verifier refuses a public input that isn't a canonical field element, so
        // `nullifier + r` can't pass for `nullifier` at another receipt's address.
        if !verify(&Proof::from_le_bytes(self.data.proof), inputs) {
            return Err(VotingError::InvalidProof.into());
        }

        let bump = [self.receipt_bump];
        let seeds = [
            Seed::from(VOTED_SEED),
            Seed::from(self.accounts.poll.address().as_ref()),
            Seed::from(self.data.nullifier),
            Seed::from(&bump),
        ];
        pinocchio_system::create_account_with_minimum_balance_signed(
            self.accounts.receipt,
            0,
            &crate::ID,
            self.accounts.payer,
            None,
            &[Signer::from(&seeds)],
        )?;

        Poll::load_mut(&mut self.accounts.poll.try_borrow_mut()?)?.count(self.data.choice)
    }
}

/// The circuit's public `Ballot`: `root`, `poll` and `options` from the poll's
/// account (its id, a u64, is the circuit's `poll`), `nullifier` and `choice` from
/// the instruction.
pub fn public_inputs(poll: &Poll, nullifier: &[u8; 32], choice: u8) -> PublicInputs {
    PublicInputs {
        ballot: Ballot {
            root: poll.root,
            poll: fields::uint(u64::from_le_bytes(poll.id).into()),
            nullifier: *nullifier,
            options: poll.options,
            choice,
        },
    }
}
