use pinocchio::{AccountView, Address, ProgramResult, error::ProgramError};

use crate::helpers::writable;

pub const POLL_SEED: &[u8] = b"poll";
pub const VOTED_SEED: &[u8] = b"voted";

/// The most options a poll has room to count. The circuit takes any `u8`; the
/// program fixes the tally's size.
pub const MAX_OPTIONS: usize = 8;

/// A poll's account, 144 bytes. Every field is a byte array, so the struct has
/// alignment 1 and every bit pattern is valid: the account's data is the struct.
///
/// | offset | size | field |
/// |---|---|---|
/// | 0 | 1 | tag, [`Poll::TAG`] |
/// | 1 | 1 | bump |
/// | 2 | 1 | options, 2 to [`MAX_OPTIONS`] |
/// | 3 | 1 | open: 1, or 0 once closed |
/// | 4 | 4 | reserved, zero |
/// | 8 | 8 | id (u64 LE) |
/// | 16 | 32 | authority |
/// | 48 | 32 | root, a little-endian field element |
/// | 80 | 64 | tally: 8 × u64 LE, one per option |
#[repr(C)]
pub struct Poll {
    pub tag: u8,
    pub bump: u8,
    pub options: u8,
    open: u8,
    reserved: [u8; 4],
    pub id: [u8; 8],
    pub authority: Address,
    pub root: [u8; 32],
    tally: [[u8; 8]; MAX_OPTIONS],
}

const _: () = assert!(Poll::LEN == 144 && align_of::<Poll>() == 1);

impl Poll {
    pub const LEN: usize = size_of::<Self>();
    pub const TAG: u8 = 1;

    /// The poll `data` holds, if it is a poll's size and tag.
    pub fn load(data: &[u8]) -> Result<&Self, ProgramError> {
        if data.len() != Self::LEN || data[0] != Self::TAG {
            return Err(ProgramError::InvalidAccountData);
        }
        // SAFETY: `Poll` is plain bytes and `data` is exactly its size.
        Ok(unsafe { &*data.as_ptr().cast::<Self>() })
    }

    /// As [`Poll::load`], to write.
    pub fn load_mut(data: &mut [u8]) -> Result<&mut Self, ProgramError> {
        if data.len() != Self::LEN || data[0] != Self::TAG {
            return Err(ProgramError::InvalidAccountData);
        }
        // SAFETY: as above.
        Ok(unsafe { &mut *data.as_mut_ptr().cast::<Self>() })
    }

    /// Write a new, open poll into `data`, a freshly created (zeroed) account: every
    /// tally starts at zero.
    pub fn init(
        data: &mut [u8],
        id: [u8; 8],
        authority: &Address,
        root: &[u8; 32],
        options: u8,
        bump: u8,
    ) -> ProgramResult {
        if data.len() != Self::LEN {
            return Err(ProgramError::InvalidAccountData);
        }
        // SAFETY: as above.
        let poll = unsafe { &mut *data.as_mut_ptr().cast::<Self>() };
        poll.tag = Self::TAG;
        poll.bump = bump;
        poll.options = options;
        poll.open = 1;
        poll.id = id;
        poll.authority = *authority;
        poll.root = *root;
        Ok(())
    }

    /// A poll account: writable, owned by this program, a poll's size and tag, at
    /// the address its own id and stored bump derive (`["poll", id]`). The bump is
    /// the canonical one `create_poll` found, so this is one hash, not a search.
    pub fn check(account: &AccountView) -> ProgramResult {
        writable(account)?;
        if !account.owned_by(&crate::ID) {
            return Err(ProgramError::InvalidAccountOwner);
        }
        let data = account.try_borrow()?;
        let poll = Self::load(&data)?;
        let address = Address::derive_address(&[POLL_SEED, &poll.id], Some(poll.bump), &crate::ID);
        if account.address() != &address {
            return Err(ProgramError::InvalidSeeds);
        }
        Ok(())
    }

    pub fn is_open(&self) -> bool {
        self.open == 1
    }

    pub fn close(&mut self) {
        self.open = 0;
    }

    /// The votes for option `choice` so far.
    pub fn tally(&self, choice: usize) -> u64 {
        self.tally.get(choice).map_or(0, |t| u64::from_le_bytes(*t))
    }

    /// Count one vote for `choice`. The proof has already bound `choice` below the
    /// poll's options, and `create_poll` the options to the tally's size.
    pub fn count(&mut self, choice: u8) -> ProgramResult {
        let tally = self
            .tally
            .get_mut(usize::from(choice))
            .ok_or(ProgramError::InvalidInstructionData)?;
        let votes = u64::from_le_bytes(*tally)
            .checked_add(1)
            .ok_or(ProgramError::ArithmeticOverflow)?;
        *tally = votes.to_le_bytes();
        Ok(())
    }
}
