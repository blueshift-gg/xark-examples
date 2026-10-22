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
    let born_by = field(adult_born_by(yyyymmdd(Clock::get()?.unix_timestamp)));
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

/// The latest birth date of someone 18 on `today` (both `YYYYMMDD`): the circuit's
/// `Policy::adult_on(today).born_by`, which is xark's `Date::years_before(18)`. The
/// same day 18 years earlier, or the 28th for a 29 February that year doesn't have.
fn adult_born_by(today: u32) -> u32 {
    let (year, month, day) = (today / 10_000 - 18, today / 100 % 100, today % 100);
    let leap = (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400);
    let day = if month == 2 && day == 29 && !leap {
        28
    } else {
        day
    };
    year * 10_000 + month * 100 + day
}

/// The UTC day of a Unix time as the circuit's `Date`, the integer `YYYYMMDD`.
/// Howard Hinnant's `civil_from_days`, from "chrono-Compatible Low-Level Date
/// Algorithms" (http://howardhinnant.github.io/date_algorithms.html).
fn yyyymmdd(unix: i64) -> u32 {
    let z = unix.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11], from March
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year * 10_000 + month * 100 + day) as u32
}

#[cfg(test)]
mod tests {
    use super::{adult_born_by, yyyymmdd};
    use age_verification::Policy;
    use xark::Date;

    /// For every day from 1900 to 2200, the program's `born_by` is the circuit's.
    #[test]
    fn born_by_is_the_circuits() {
        let start = -2_208_988_800; // 1900-01-01
        for n in 0..(300 * 366) {
            let today = yyyymmdd(start + n * 86_400);
            let (y, m, d) = (today / 10_000, today / 100 % 100, today % 100);
            let date = Date::new(y as u16, m as u8, d as u8);
            let (by, bm, bd) = Policy::adult_on(date).born_by.parts();
            let want = u32::from(by) * 10_000 + u32::from(bm) * 100 + u32::from(bd);
            assert_eq!(adult_born_by(today), want, "on {today}");
        }
    }

    /// Anchors from Python's `datetime(y, m, d, tzinfo=timezone.utc).timestamp()`.
    #[test]
    fn days_are_calendar_days() {
        assert_eq!(yyyymmdd(0), 19700101);
        assert_eq!(yyyymmdd(86_399), 19700101);
        assert_eq!(yyyymmdd(-1), 19691231);
        assert_eq!(yyyymmdd(951_868_800), 20000301);
        assert_eq!(yyyymmdd(1_709_164_800), 20240229);
        assert_eq!(yyyymmdd(1_792_627_200), 20261022);
    }

    /// Consecutive days from 1900 to 2200 step through the calendar one day at a
    /// time: the day goes up by one, or it's the first of the next month (with the
    /// last day of the month it left right for leap years), or of the next year.
    #[test]
    fn every_day_follows_the_one_before() {
        let parts = |d: u32| (d / 10_000, d / 100 % 100, d % 100);
        let leap =
            |y: u32| (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400);
        let last = |y: u32, m: u32| match m {
            2 if leap(y) => 29,
            2 => 28,
            4 | 6 | 9 | 11 => 30,
            _ => 31,
        };
        let start = -2_208_988_800; // 1900-01-01
        let mut prev = parts(yyyymmdd(start));
        assert_eq!(prev, (1900, 1, 1));
        for n in 1..(300 * 366) {
            let (y, m, d) = parts(yyyymmdd(start + n * 86_400));
            let (py, pm, pd) = prev;
            let next = if pd < last(py, pm) {
                (py, pm, pd + 1)
            } else if pm < 12 {
                (py, pm + 1, 1)
            } else {
                (py + 1, 1, 1)
            };
            assert_eq!((y, m, d), next);
            prev = (y, m, d);
        }
    }
}
