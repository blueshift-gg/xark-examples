use pinocchio::error::{ProgramError, ToStr};

/// The failures the runtime's `ProgramError` has no name for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum ShieldedTransferError {
    /// The proof doesn't verify against the public inputs the program assembled.
    InvalidProof = 0,
    /// Not one of the last `ROOT_HISTORY_SIZE` roots.
    UnknownRoot = 1,
    TreeFull = 2,
    /// A deposit (`vpub_in > 0`) needs `user_token`.
    NoSource = 3,
    /// A withdrawal (`vpub_out > 0`) needs `recipient_token`.
    NoRecipient = 4,
    /// A fee (`fee > 0`) needs `relayer_token`.
    NoRelayer = 5,
    /// The nullifier's account exists: the note was spent.
    NullifierSpent = 6,
    /// A token account of another mint than the pool's.
    WrongMint = 7,
}

impl From<ShieldedTransferError> for ProgramError {
    fn from(e: ShieldedTransferError) -> Self {
        ProgramError::Custom(e as u32)
    }
}

impl TryFrom<u32> for ShieldedTransferError {
    type Error = ProgramError;

    fn try_from(code: u32) -> Result<Self, Self::Error> {
        Ok(match code {
            0 => Self::InvalidProof,
            1 => Self::UnknownRoot,
            2 => Self::TreeFull,
            3 => Self::NoSource,
            4 => Self::NoRecipient,
            5 => Self::NoRelayer,
            6 => Self::NullifierSpent,
            7 => Self::WrongMint,
            _ => return Err(ProgramError::InvalidArgument),
        })
    }
}

impl ToStr for ShieldedTransferError {
    fn to_str(&self) -> &'static str {
        match self {
            Self::InvalidProof => "InvalidProof",
            Self::UnknownRoot => "UnknownRoot",
            Self::TreeFull => "TreeFull",
            Self::NoSource => "NoSource",
            Self::NoRecipient => "NoRecipient",
            Self::NoRelayer => "NoRelayer",
            Self::NullifierSpent => "NullifierSpent",
            Self::WrongMint => "WrongMint",
        }
    }
}
