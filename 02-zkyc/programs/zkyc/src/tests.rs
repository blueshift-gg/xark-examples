use crate::policy::*;
use xark::Field;
use xark::prelude::{Bitfield, Country::*};
use zkyc::Policy;
use zkyc_verifier::{Proof, PublicInputs, verify};

/// The circuit's own `Policy`, built with its Rust: what the program's must equal.
fn circuits(today: zkyc::Day) -> Policy {
    Policy::default()
        .on(today)
        .min_age(MIN_AGE as u16)
        .worth_at_least(MIN_NET_WORTH)
        .nationalities([US, GB, FR, DE])
}

/// For every day from 1918 (the first whose `born_by` is in the circuit's range)
/// to 2100, the program's policy is the circuit's, field element for field
/// element: the dates, and the generated crate's packing of them.
#[test]
fn the_policy_is_the_circuits() {
    let start = days_from_civil(1918, 1, 1) * 86_400;
    let end = days_from_civil(2101, 1, 1) * 86_400;
    for unix in (start..end).step_by(86_400) {
        let today = zkyc::Day::from_unix(unix).unwrap();
        let want = circuits(today);
        let programs = policy(unix).unwrap();
        assert_eq!(programs.today.steps(), want.today.steps());
        assert_eq!(
            programs.born_by.steps(),
            want.born_by.steps(),
            "on {:?}",
            today.date()
        );
        let fields = PublicInputs {
            commitment: [0; 32],
            policy: programs,
            holder: [0; 32],
        }
        .to_fields();
        let got = [
            Field::from_le_bytes(&fields[1]),
            Field::from_le_bytes(&fields[2]),
        ];
        assert_eq!(got, want.to_fields(), "on {:?}", today.date());
        // Any moment of the day is that day.
        assert_eq!(policy(unix + 86_399), Some(programs));
    }
}

/// `ALLOWED` is DE, FR, GB and US as the circuit's `Bitfield<Country>` counts
/// them, and no others.
#[test]
fn allowed_is_four_countries() {
    let want = Bitfield::of([DE, FR, GB, US]);
    assert_eq!(Field::from_le_bytes(&ALLOWED.to_field()), want.field());
}

/// No policy outside the circuit's days: a `born_by` before 1900, or after 2100.
#[test]
fn no_policy_outside_the_circuits_days() {
    assert!(policy(days_from_civil(1917, 12, 31) * 86_400).is_none());
    assert!(policy(days_from_civil(1918, 1, 1) * 86_400).is_some());
    assert!(policy(days_from_civil(2100, 12, 31) * 86_400).is_some());
    assert!(policy(days_from_civil(2101, 1, 1) * 86_400).is_none());
}

/// The demo proof (`examples/demo.rs`: holder `[7; 32]`, 2026-10-22) verifies
/// against the public inputs the program assembles, for its holder and day only.
#[test]
fn the_demo_proof_verifies_for_its_holder_and_day() {
    let proof = include_bytes!("../../../clients/zkyc/rust/proof.solana.bin");
    let inputs = include_bytes!("../../../clients/zkyc/rust/public_inputs.solana.bin");
    let proof = Proof::from_le_bytes(proof);
    let commitment: &[u8; 32] = inputs.first_chunk().unwrap();
    let noon = days_from_civil(2026, 10, 22) * 86_400 + 43_200;
    assert!(verify(
        &proof,
        public_inputs(commitment, noon, &[7; 32]).unwrap()
    ));
    assert!(!verify(
        &proof,
        public_inputs(commitment, noon, &[8; 32]).unwrap()
    ));
    assert!(!verify(
        &proof,
        public_inputs(commitment, noon + 86_400, &[7; 32]).unwrap()
    ));
    assert!(!verify(
        &proof,
        public_inputs(&[1; 32], noon, &[7; 32]).unwrap()
    ));
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

/// The client tests match the name the program logs for its error.
#[test]
fn errors_log_their_names() {
    let error = pinocchio::error::ProgramError::from(crate::ZkycError::InvalidProof);
    assert!(
        error
            .to_str::<crate::ZkycError>()
            .starts_with("InvalidProof")
    );
}
