use pinocchio::error::{ProgramError, ToStr};

/// The program's own errors. Everything else it refuses has a built-in
/// `ProgramError`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum TokenClaimError {
    /// The proof doesn't verify against the root, the signer and the amount.
    InvalidProof = 0,
}

impl From<TokenClaimError> for ProgramError {
    fn from(error: TokenClaimError) -> Self {
        ProgramError::Custom(error as u32)
    }
}

impl TryFrom<u32> for TokenClaimError {
    type Error = ProgramError;

    fn try_from(code: u32) -> Result<Self, Self::Error> {
        match code {
            0 => Ok(TokenClaimError::InvalidProof),
            _ => Err(ProgramError::InvalidArgument),
        }
    }
}

impl ToStr for TokenClaimError {
    fn to_str(&self) -> &'static str {
        match self {
            TokenClaimError::InvalidProof => "InvalidProof: invalid proof",
        }
    }
}
