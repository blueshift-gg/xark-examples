use pinocchio::{AccountView, ProgramResult, error::ProgramError};

/// A writable signer: the payer of a new account.
pub fn signer(account: &AccountView) -> ProgramResult {
    if !account.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }
    writable(account)
}

pub fn writable(account: &AccountView) -> ProgramResult {
    if !account.is_writable() {
        return Err(ProgramError::Immutable);
    }
    Ok(())
}

/// The token program and the system program, by address.
pub fn programs(token_program: &AccountView, system_program: &AccountView) -> ProgramResult {
    if token_program.address() != &pinocchio_token::ID
        || system_program.address() != &pinocchio_system::ID
    {
        return Err(ProgramError::IncorrectProgramId);
    }
    Ok(())
}

/// `Program log: <message>`.
pub fn log(message: &str) {
    #[cfg(any(target_os = "solana", target_arch = "bpf"))]
    // SAFETY: the pointer and length are a live `&str`'s.
    unsafe {
        pinocchio::syscalls::sol_log_(message.as_ptr(), message.len() as u64)
    };
    #[cfg(not(any(target_os = "solana", target_arch = "bpf")))]
    let _ = message;
}
