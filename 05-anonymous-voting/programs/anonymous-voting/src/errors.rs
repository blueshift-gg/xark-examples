use pinocchio::error::{ProgramError, ToStr};

/// The program's own errors, as `ProgramError::Custom` codes. Every other failure
/// is a built-in: a missing signature, a wrong owner or address, malformed data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum VotingError {
    /// The proof doesn't verify against the ballot the program assembled: another
    /// poll's, another list's, another choice's, or no voter's at all.
    InvalidProof = 0,
    /// The nullifier's receipt exists: this voter has voted in this poll.
    AlreadyVoted,
    /// The poll's authority has closed it.
    PollClosed,
}

impl From<VotingError> for ProgramError {
    fn from(e: VotingError) -> Self {
        ProgramError::Custom(e as u32)
    }
}

impl TryFrom<u32> for VotingError {
    type Error = ProgramError;

    fn try_from(code: u32) -> Result<Self, Self::Error> {
        match code {
            0 => Ok(Self::InvalidProof),
            1 => Ok(Self::AlreadyVoted),
            2 => Ok(Self::PollClosed),
            _ => Err(ProgramError::InvalidArgument),
        }
    }
}

impl ToStr for VotingError {
    fn to_str(&self) -> &'static str {
        match self {
            Self::InvalidProof => "InvalidProof: invalid proof",
            Self::AlreadyVoted => "AlreadyVoted: this nullifier has voted in this poll",
            Self::PollClosed => "PollClosed: the poll is closed",
        }
    }
}
