//! Onchain verifier for the zkyc circuit (Pinocchio).
//! Accounts: the holder, a signer. Instruction data: proof (256 B) ‖ commitment
//! (32 B, little-endian). The program supplies every other public input: its own
//! policy, checked on today's date from the Clock sysvar, and the holder's address.
//! The verifier picks the rules, not the prover: a proof made against a laxer
//! policy, for another day, or for another wallet doesn't verify. A real deployment
//! would also check the commitment against an issuer's registry (see README).
#![cfg_attr(not(test), no_std)]

use pinocchio::{
    AccountView, Address, ProgramResult,
    error::ProgramError,
    program_entrypoint,
    sysvars::{Sysvar, clock::Clock},
};
use zkyc_xark_verifier::{Proof, verify};

program_entrypoint!(process_instruction);
#[cfg(not(test))]
pinocchio::nostd_panic_handler!();
#[cfg(not(test))]
pinocchio::no_allocator!();

// ---- the program's policy ----------------------------------------------------------

/// At least this old today.
const MIN_AGE: i64 = 18;

/// The least attested net worth, in the attestor's unit (its currency and scale).
const MIN_NET_WORTH: u64 = 500_000;

/// The nationalities allowed: DE, FR, GB and US, by their positions in xark's
/// `Country` (the ISO 3166-1 alpha-2 codes, alphabetically; the tests check them).
const ALLOWED: [u8; 32] = countries(&[56, 74, 76, 232]);

fn process_instruction(
    _program_id: &Address,
    accounts: &mut [AccountView],
    instruction_data: &[u8],
) -> ProgramResult {
    let Some((proof, commitment)) = instruction_data.split_first_chunk::<256>() else {
        return Err(ProgramError::InvalidInstructionData);
    };
    let commitment: &[u8; 32] = commitment
        .try_into()
        .map_err(|_| ProgramError::InvalidInstructionData)?;
    let [holder, ..] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    if !holder.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }
    let unix = Clock::get()?.unix_timestamp;
    let public_inputs = public_inputs(commitment, unix, holder.address().as_array());
    if verify(&Proof::from_le_bytes(proof), &public_inputs) {
        Ok(())
    } else {
        Err(ProgramError::InvalidInstructionData)
    }
}

/// The circuit's public inputs, in its order (`xark inspect --json`): `commitment`;
/// `policy` as two field elements, its packed `{today, born_by, min_net_worth}` and
/// `allowed`; `holder` as `[hi, lo]`.
fn public_inputs(commitment: &[u8; 32], unix: i64, holder: &[u8; 32]) -> [[u8; 32]; 5] {
    let (policy, allowed) = policy(unix);
    let [holder_hi, holder_lo] = halves(holder);
    [*commitment, policy, allowed, holder_hi, holder_lo]
}

/// The program's `Policy` on the day holding the Unix time `unix`, as the circuit
/// takes it. `#[input]` packs a struct's consecutive fixed-width fields into one field
/// element, the first in the lowest bits, while they fit in 253 bits: `today` (a
/// 17-bit day) in bits 0–16, `born_by` in 17–33 and `min_net_worth` (64 bits) in
/// 34–97. `allowed`, a `Bitfield` of 249 countries, is a field element of its own,
/// bit `i` for the country at position `i`. Each as a 32-byte little-endian field
/// element.
fn policy(unix: i64) -> ([u8; 32], [u8; 32]) {
    let day = unix.div_euclid(86_400);
    let today = since_1900(day);
    let born_by = born_by(day);
    let packed = u128::from(today) | u128::from(born_by) << 17 | u128::from(MIN_NET_WORTH) << 34;
    let mut out = [0u8; 32];
    out[..16].copy_from_slice(&packed.to_le_bytes());
    (out, ALLOWED)
}

/// A `Bitfield<Country>` of the countries at `positions`: bit `i` set for each.
const fn countries(positions: &[usize]) -> [u8; 32] {
    let mut out = [0u8; 32];
    let mut i = 0;
    while i < positions.len() {
        out[positions[i] / 8] |= 1 << (positions[i] % 8);
        i += 1;
    }
    out
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

// ---- dates -------------------------------------------------------------------------

/// A day (counted from 1970-01-01) as the circuit's `Day` (`Days<1900, 2100>`) holds
/// it: days since 1900-01-01.
fn since_1900(day: i64) -> u32 {
    (day - days_from_civil(1900, 1, 1)) as u32
}

/// The circuit's `min_age(MIN_AGE)` on `day`: the latest birth date of someone
/// `MIN_AGE` that day. The same day `MIN_AGE` years earlier, or the 28th for a
/// 29 February that year doesn't have, as xark's `years_before` does.
fn born_by(day: i64) -> u32 {
    let (y, m, d) = civil_from_days(day);
    let y = y - MIN_AGE;
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let d = if m == 2 && d == 29 && !leap { 28 } else { d };
    since_1900(days_from_civil(y, m, d))
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
    use super::*;
    use xark::Field;
    use xark::prelude::{Bitfield, Country::*};
    use zkyc::{Day, Policy};

    /// The circuit's own `Policy`, built with its Rust: what the program's must equal.
    fn circuits(today: Day) -> Policy {
        Policy::default()
            .on(today)
            .min_age(MIN_AGE as u16)
            .worth_at_least(MIN_NET_WORTH)
            .nationalities([US, GB, FR, DE])
    }

    /// For every day from 1918 (the first whose `born_by` is in the circuit's range)
    /// to 2100, the program's policy is the circuit's, field element for field
    /// element: the dates, and the packing.
    #[test]
    fn the_policy_is_the_circuits() {
        let start = days_from_civil(1918, 1, 1) * 86_400;
        let end = days_from_civil(2101, 1, 1) * 86_400;
        for unix in (start..end).step_by(86_400) {
            let today = Day::from_unix(unix).unwrap();
            let want = circuits(today);
            let day = unix.div_euclid(86_400);
            assert_eq!(u64::from(since_1900(day)), want.today.steps());
            assert_eq!(
                u64::from(born_by(day)),
                want.born_by.steps(),
                "on {:?}",
                today.date()
            );
            let (packed, allowed) = policy(unix);
            let got = [
                Field::from_le_bytes(&packed),
                Field::from_le_bytes(&allowed),
            ];
            assert_eq!(got, want.to_fields(), "on {:?}", today.date());
            // Any moment of the day is that day.
            assert_eq!(policy(unix + 86_399), policy(unix));
        }
    }

    /// The positions in `ALLOWED` are DE, FR, GB and US, and no others.
    #[test]
    fn allowed_is_four_countries() {
        let want = Bitfield::of([DE, FR, GB, US]);
        assert_eq!(Field::from_le_bytes(&ALLOWED), want.field());
    }

    /// The demo proof (`examples/demo.rs`: holder `[7; 32]`, 2026-10-22) verifies
    /// against the public inputs the program assembles, for its holder and day only.
    #[test]
    fn the_demo_proof_verifies_for_its_holder_and_day() {
        let proof = include_bytes!("../../circuit/zkyc-verifier/proof.solana.bin");
        let inputs = include_bytes!("../../circuit/zkyc-verifier/public_inputs.solana.bin");
        let proof = Proof::from_le_bytes(proof);
        let commitment: &[u8; 32] = inputs.first_chunk().unwrap();
        let noon = days_from_civil(2026, 10, 22) * 86_400 + 43_200;
        assert!(verify(&proof, &public_inputs(commitment, noon, &[7; 32])));
        assert!(!verify(&proof, &public_inputs(commitment, noon, &[8; 32])));
        assert!(!verify(
            &proof,
            &public_inputs(commitment, noon + 86_400, &[7; 32])
        ));
        assert!(!verify(&proof, &public_inputs(&[1; 32], noon, &[7; 32])));
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
