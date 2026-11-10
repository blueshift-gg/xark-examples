//! The program's policy, packed as the circuit takes it, and the calendar it needs.

/// At least this old today.
pub const MIN_AGE: i64 = 18;

/// The least attested net worth, in the attestor's unit (its currency and scale).
pub const MIN_NET_WORTH: u64 = 500_000;

/// The nationalities allowed: DE, FR, GB and US, by their positions in xark's
/// `Country` (the ISO 3166-1 alpha-2 codes, alphabetically; the tests check them).
pub const ALLOWED: [u8; 32] = countries(&[56, 74, 76, 232]);

/// The circuit's public inputs, in its order (`xark inspect --json`): `commitment`;
/// `policy` as two field elements, its packed `{today, born_by, min_net_worth}` and
/// `allowed`; `holder` as `[hi, lo]`.
pub fn public_inputs(commitment: &[u8; 32], unix: i64, holder: &[u8; 32]) -> [[u8; 32]; 5] {
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
pub fn policy(unix: i64) -> ([u8; 32], [u8; 32]) {
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

/// A day (counted from 1970-01-01) as the circuit's `Day` (`Days<1900, 2100>`) holds
/// it: days since 1900-01-01.
pub fn since_1900(day: i64) -> u32 {
    (day - days_from_civil(1900, 1, 1)) as u32
}

/// The circuit's `min_age(MIN_AGE)` on `day`: the latest birth date of someone
/// `MIN_AGE` that day. The same day `MIN_AGE` years earlier, or the 28th for a
/// 29 February that year doesn't have, as xark's `years_before` does.
pub fn born_by(day: i64) -> u32 {
    let (y, m, d) = civil_from_days(day);
    let y = y - MIN_AGE;
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let d = if m == 2 && d == 29 && !leap { 28 } else { d };
    since_1900(days_from_civil(y, m, d))
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
