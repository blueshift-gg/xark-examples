use pinocchio::error::{ProgramError, ToStr};

/// The program's own errors, as `ProgramError::Custom` codes. Every other failure
/// is a built-in: a missing signature, short accounts, malformed instruction data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum ZkycError {
    /// The proof doesn't verify against the commitment, the program's policy today
    /// and the signer: another wallet's, another day's, or a laxer policy's.
    InvalidProof = 0,
}

impl From<ZkycError> for ProgramError {
    fn from(e: ZkycError) -> Self {
        ProgramError::Custom(e as u32)
    }
}

impl TryFrom<u32> for ZkycError {
    type Error = ProgramError;

    fn try_from(code: u32) -> Result<Self, Self::Error> {
        match code {
            0 => Ok(Self::InvalidProof),
            _ => Err(ProgramError::InvalidArgument),
        }
    }
}

impl ToStr for ZkycError {
    fn to_str(&self) -> &'static str {
        match self {
            Self::InvalidProof => "InvalidProof: invalid proof",
        }
    }
}
