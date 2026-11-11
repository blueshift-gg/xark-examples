//! The program's policy, as the circuit's `Policy`, and the calendar a minimum age
//! needs. The generated crate packs it as the circuit takes it.
use zkyc_verifier::{Bitfield, Country, Days, Policy, PublicInputs};

/// A day as the circuit's `Day`: 1900-01-01 to 2100-12-31.
pub type Day = Days<1900, 2100>;

/// At least this old today.
pub const MIN_AGE: i64 = 18;

/// The least attested net worth, in the attestor's unit (its currency and scale).
pub const MIN_NET_WORTH: u64 = 500_000;

/// The nationalities allowed.
pub const ALLOWED: Bitfield<Country> = Bitfield::EMPTY
    .with(Country::DE)
    .with(Country::FR)
    .with(Country::GB)
    .with(Country::US);

/// The circuit's public inputs for `holder` and `commitment` under the program's
/// policy on the day holding the Unix time `unix`, or `None` on a day the circuit
/// can't hold (its `born_by` before 1900, or `today` after 2100).
pub fn public_inputs(commitment: &[u8; 32], unix: i64, holder: &[u8; 32]) -> Option<PublicInputs> {
    Some(PublicInputs {
        commitment: *commitment,
        policy: policy(unix)?,
        holder: *holder,
    })
}

/// The program's `Policy` on the day holding the Unix time `unix`.
pub fn policy(unix: i64) -> Option<Policy> {
    let today = Day::from_unix(unix)?;
    Some(Policy {
        today,
        born_by: born_by(today)?,
        min_net_worth: MIN_NET_WORTH,
        allowed: ALLOWED,
    })
}

/// The circuit's `min_age(MIN_AGE)` on `today`: the latest birth date of someone
/// `MIN_AGE` that day. The same day `MIN_AGE` years earlier, or the 28th for a
/// 29 February that year doesn't have, as xark's `years_before` does.
pub fn born_by(today: Day) -> Option<Day> {
    let (y, m, d) = civil_from_days(today.to_unix().div_euclid(86_400));
    let y = y - MIN_AGE;
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let d = if m == 2 && d == 29 && !leap { 28 } else { d };
    Day::from_unix(days_from_civil(y, m, d) * 86_400)
}

/// The civil date of a day count from 1970-01-01 (UTC). Howard Hinnant's
/// `civil_from_days`, from "chrono-Compatible Low-Level Date Algorithms"
/// (http://howardhinnant.github.io/date_algorithms.html).
pub fn civil_from_days(z: i64) -> (i64, i64, i64) {
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
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400; // [0, 399]
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}
