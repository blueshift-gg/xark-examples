use pinocchio::{
    AccountView, Address, ProgramResult,
    cpi::{Seed, Signer},
    sysvars::{Sysvar, rent::Rent},
};
use pinocchio_system::instructions::{Allocate, Assign, CreateAccount, Transfer};

/// Create the PDA `account` (signed by `seeds`), `space` bytes owned by `owner`, paid
/// by `payer`. An account anyone can send lamports to could otherwise be blocked
/// that way: `CreateAccount` refuses an address that holds any. So an account that
/// does is topped up to rent-exempt and assigned instead (a receipt or a distributor
/// that's still the system program's has no data, and only its seeds can sign).
pub fn create(
    payer: &AccountView,
    account: &AccountView,
    space: usize,
    owner: &Address,
    seeds: &[Seed],
) -> ProgramResult {
    let signer = [Signer::from(seeds)];
    let rent = Rent::get()?.try_minimum_balance(space)?;
    let held = account.lamports();
    if held == 0 {
        return CreateAccount {
            from: payer,
            to: account,
            lamports: rent,
            space: space as u64,
            owner,
        }
        .invoke_signed(&signer);
    }
    if held < rent {
        Transfer {
            from: payer,
            to: account,
            lamports: rent - held,
        }
        .invoke()?;
    }
    if space > 0 {
        Allocate {
            account,
            space: space as u64,
        }
        .invoke_signed(&signer)?;
    }
    Assign { account, owner }.invoke_signed(&signer)
}

/// Write `message` to the program's logs.
pub fn log(message: &str) {
    #[cfg(target_os = "solana")]
    // SAFETY: the pointer and length are a valid `&str`'s.
    unsafe {
        pinocchio::syscalls::sol_log_(message.as_ptr(), message.len() as u64)
    };
    #[cfg(not(target_os = "solana"))]
    let _ = message;
}
