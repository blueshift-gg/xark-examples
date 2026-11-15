//! The program's policy, as the circuit's `Policy`, and the calendar a minimum age
//! needs. The generated crate packs it as the circuit takes it.
use time::OffsetDateTime;
use zkyc_verifier::{Bitfield, Country, Days, Policy, PublicInputs};

/// A day as the circuit's `Day`: 1900-01-01 to 2100-12-31.
pub type Day = Days<1900, 2100>;

/// At least this old today.
pub const MIN_AGE: i32 = 18;

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
/// 29 February that year doesn't have, as xark's `years_before` does (`time`
/// refuses a day the year doesn't have, rather than moving it).
pub fn born_by(today: Day) -> Option<Day> {
    let today = OffsetDateTime::from_unix_timestamp(today.to_unix())
        .ok()?
        .date();
    let year = today.year() - MIN_AGE;
    let born_by = today
        .replace_year(year)
        .or_else(|_| today.replace_day(28)?.replace_year(year))
        .ok()?;
    Day::from_unix(born_by.midnight().assume_utc().unix_timestamp())
}
