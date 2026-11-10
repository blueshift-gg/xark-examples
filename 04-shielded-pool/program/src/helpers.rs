use pinocchio::{AccountView, ProgramResult, error::ProgramError};

/// An account the program writes to.
pub fn writable(account: &AccountView) -> ProgramResult {
    if !account.is_writable() {
        return Err(ProgramError::Immutable);
    }
    Ok(())
}

/// A payer: a signer, writable.
pub fn writable_signer(account: &AccountView) -> ProgramResult {
    if !account.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }
    writable(account)
}

/// The system program, which creates the accounts and moves the deposits.
pub fn system_program(account: &AccountView) -> ProgramResult {
    if account.address() != &pinocchio_system::ID {
        return Err(ProgramError::IncorrectProgramId);
    }
    Ok(())
}

/// An account about to be created: nothing lives at the address yet. A
/// system-owned account with no data may hold lamports (anyone can send them),
/// which creation tops up to rent exemption rather than failing on.
pub fn is_uninitialized(account: &AccountView) -> bool {
    account.owned_by(&pinocchio_system::ID) && account.is_data_empty()
}

/// A u64 as a 32-byte little-endian field element.
pub fn field_from_u64(x: u64) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[..8].copy_from_slice(&x.to_le_bytes());
    out
}

/// `Program log: <message>`.
pub fn log(message: &str) {
    #[cfg(any(target_os = "solana", target_arch = "bpf"))]
    // SAFETY: the pointer and length are a valid `&str`'s.
    unsafe {
        pinocchio::syscalls::sol_log_(message.as_ptr(), message.len() as u64)
    };
    #[cfg(not(any(target_os = "solana", target_arch = "bpf")))]
    let _ = message;
}

/// `Program data: <base64> <base64> ..`, one per slice.
pub fn log_data(data: &[&[u8]]) {
    #[cfg(any(target_os = "solana", target_arch = "bpf"))]
    // SAFETY: `data` is a valid slice of slices, the layout the syscall reads.
    unsafe {
        pinocchio::syscalls::sol_log_data(data.as_ptr().cast(), data.len() as u64)
    };
    #[cfg(not(any(target_os = "solana", target_arch = "bpf")))]
    let _ = data;
}
