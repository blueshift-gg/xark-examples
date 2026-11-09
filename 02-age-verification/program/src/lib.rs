//! On-chain verifier for the age-verification circuit (Pinocchio).
//! Accounts: the holder, a signer. Instruction data: proof (256 B) ‖ commitment
//! (32 B, little-endian). The program supplies the other public inputs: the
//! circuit's `Policy::adult_on(today)`, from the Clock sysvar, and the holder's
//! address. A proof made for another day, or for another wallet, doesn't verify. A
//! real deployment would also check the commitment against an issuer's registry
//! (see README).
#![cfg_attr(not(test), no_std)]

use age_verification_xark_verifier::{Proof, verify};
use pinocchio::{
    AccountView, Address, ProgramResult,
    error::ProgramError,
    program_entrypoint,
    sysvars::{Sysvar, clock::Clock},
};

program_entrypoint!(process_instruction);
#[cfg(not(test))]
pinocchio::nostd_panic_handler!();
#[cfg(not(test))]
pinocchio::no_allocator!();

fn process_instruction(
    _program_id: &Address,
    accounts: &mut [AccountView],
    instruction_data: &[u8],
) -> ProgramResult {
    let Some((proof, commitment)) = instruction_data.split_first_chunk::<256>() else {
        return Err(ProgramError::InvalidInstructionData);
    };
    let commitment: [u8; 32] = commitment
        .try_into()
        .map_err(|_| ProgramError::InvalidInstructionData)?;
    let [holder, ..] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    if !holder.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }
    let born_by = field(born_by(Clock::get()?.unix_timestamp));
    let [holder_hi, holder_lo] = halves(holder.address().as_array());
    let public_inputs = [commitment, born_by, holder_hi, holder_lo];
    if verify(&Proof::from_le_bytes(proof), &public_inputs) {
        Ok(())
    } else {
        Err(ProgramError::InvalidInstructionData)
    }
}

/// An address as the circuit's `Hash`: `[hi, lo]`, each 16-byte half read as a
/// big-endian integer, as 32-byte little-endian field elements.
fn halves(bytes: &[u8; 32]) -> [[u8; 32]; 2] {
    let half = |b: &[u8]| {
        let mut out = [0u8; 32];
        out[..16].copy_from_slice(b);
        out[..16].reverse();
        out
    };
    [half(&bytes[..16]), half(&bytes[16..])]
}

/// A `u32` as a 32-byte little-endian field element.
fn field(x: u32) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[..4].copy_from_slice(&x.to_le_bytes());
    out
}

/// The circuit's `Policy::adult_on(today).born_by` for the day holding the Unix time
/// `unix`, as the circuit's `Date` (`Days<1900, 2100>`) holds it: days since
/// 1900-01-01. That's the latest birth date of someone 18 that day: the same day 18
/// years earlier, or the 28th for a 29 February that year doesn't have, as xark's
/// `years_before` does.
fn born_by(unix: i64) -> u32 {
    let (y, m, d) = civil_from_days(unix.div_euclid(86_400));
    let y = y - 18;
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let d = if m == 2 && d == 29 && !leap { 28 } else { d };
    (days_from_civil(y, m, d) - days_from_civil(1900, 1, 1)) as u32
}

/// The civil date of a day count from 1970-01-01 (UTC). Howard Hinnant's
/// `civil_from_days`, from "chrono-Compatible Low-Level Date Algorithms"
/// (http://howardhinnant.github.io/date_algorithms.html).
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11], from March
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

/// The day count from 1970-01-01 of a civil date: Hinnant's `days_from_civil`, the
/// inverse of [`civil_from_days`].
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400; // [0, 399]
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::{born_by, civil_from_days, days_from_civil};
    use age_verification::{Date, Policy};

    /// For every day from 1918 (the first whose `born_by` is in the circuit's range)
    /// to 2100, the program's `born_by` is the circuit's.
    #[test]
    fn born_by_is_the_circuits() {
        let start = days_from_civil(1918, 1, 1) * 86_400;
        let end = days_from_civil(2101, 1, 1) * 86_400;
        for unix in (start..end).step_by(86_400) {
            let today = Date::from_unix(unix).unwrap();
            let want = Policy::adult_on(today).born_by.steps();
            assert_eq!(u64::from(born_by(unix)), want, "on {:?}", today.date());
            // Any moment of the day is that day.
            assert_eq!(born_by(unix + 86_399), born_by(unix));
        }
    }

    /// Anchors from Python's `datetime(y, m, d, tzinfo=timezone.utc).timestamp()`.
    #[test]
    fn days_are_calendar_days() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
        assert_eq!(civil_from_days(951_868_800 / 86_400), (2000, 3, 1));
        assert_eq!(civil_from_days(1_709_164_800 / 86_400), (2024, 2, 29));
        assert_eq!(civil_from_days(1_792_627_200 / 86_400), (2026, 10, 22));
        for z in -30_000..100_000 {
            let (y, m, d) = civil_from_days(z);
            assert_eq!(days_from_civil(y, m, d), z);
        }
    }
}
