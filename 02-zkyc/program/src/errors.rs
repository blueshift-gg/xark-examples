use pinocchio::error::ProgramError;

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

impl ZkycError {
    /// The name the program logs for error `code`.
    pub fn name(code: u32) -> &'static str {
        match code {
            0 => "InvalidProof",
            _ => "unknown error",
        }
    }
}
