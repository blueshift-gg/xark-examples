use pinocchio::error::{ProgramError, ToStr};

/// The program's own errors, as `ProgramError::Custom` codes. Every other failure
/// is a built-in: a missing signature, a wrong owner or address, malformed data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum ShieldedPoolError {
    /// The proof doesn't verify against the statement the program assembled.
    InvalidProof = 0,
    /// The withdrawal's root isn't one of the pool's recent roots.
    UnknownRoot,
    /// The relayer's fee is more than the note is worth.
    FeeTooHigh,
    /// Every leaf of the tree is taken.
    TreeFull,
    /// The note's nullifier account exists: it has been withdrawn.
    NullifierSpent,
}

impl From<ShieldedPoolError> for ProgramError {
    fn from(e: ShieldedPoolError) -> Self {
        ProgramError::Custom(e as u32)
    }
}

impl TryFrom<u32> for ShieldedPoolError {
    type Error = ProgramError;

    fn try_from(code: u32) -> Result<Self, Self::Error> {
        match code {
            0 => Ok(Self::InvalidProof),
            1 => Ok(Self::UnknownRoot),
            2 => Ok(Self::FeeTooHigh),
            3 => Ok(Self::TreeFull),
            4 => Ok(Self::NullifierSpent),
            _ => Err(ProgramError::InvalidArgument),
        }
    }
}

impl ToStr for ShieldedPoolError {
    fn to_str(&self) -> &'static str {
        match self {
            Self::InvalidProof => "InvalidProof: invalid proof",
            Self::UnknownRoot => "UnknownRoot: unknown or stale merkle root",
            Self::FeeTooHigh => "FeeTooHigh: fee exceeds denomination",
            Self::TreeFull => "TreeFull: tree is full",
            Self::NullifierSpent => "NullifierSpent: nullifier already spent",
        }
    }
}
