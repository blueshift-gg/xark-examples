//! Prove age >= 18 against a public commitment to an identity, revealing
//! neither the birth year nor the nonce. Public input order is `commitment`,
//! `current_year`; the on-chain program packs instruction data to match.
use xark::prelude::*;
use xark_poseidon2::hash2;

/// Private credential fields travel together in one typed circuit input.
/// `CircuitInput` derives both the in-circuit flattening and the host-side
/// witness mapping, so those layouts cannot drift apart.
#[derive(Clone, Copy, Debug, CircuitInput)]
pub struct Identity {
    pub birth_year: Field,
    pub nonce: Field,
}

#[circuit]
pub fn age_verification(
    identity: Private<Identity>,
    commitment: Public<Field>,
    current_year: Public<Field>,
) {
    // 1. Prove we know the opening of the published commitment. `hash2` is the
    //    same Poseidon2 gadget the client hashes with — params match forever.
    require_eq(hash2(identity.birth_year, identity.nonce), commitment);

    // 2. Prove adulthood. The explicit `::<32>` width range-checks both years
    //    to 32 bits, so the subtraction below cannot wrap the field; `18u32`
    //    then bounds the age the same way.
    require(current_year.ge::<32>(identity.birth_year));
    let age = current_year - identity.birth_year;
    require(age >= 18u32);
}

#[cfg(test)]
mod tests {
    use super::{Identity, age_verification};
    use xark::Field;

    // Poseidon2(birth_year, nonce) for the two test years — computed with the
    // repo's native mirror of the gadget (`cargo run --bin commit`, e2e crate).
    const NONCE: Field = Field::constant(
        "13102783505849749806854498441424493672889223651541975554449217606361432761234",
    );
    const COMMIT_1990: Field = Field::constant(
        "16270599866198939632822375879477475677301143667419388581635073777919968987292",
    );
    const COMMIT_2020: Field = Field::constant(
        "13573765523911323905052498488738541803332695318708852347910960062058857884122",
    );

    fn identity(birth_year: u64) -> Identity {
        Identity {
            birth_year: Field::from(birth_year),
            nonce: NONCE,
        }
    }

    #[test]
    fn adult_passes() {
        // 2026 - 1990 = 36 ≥ 18
        age_verification(identity(1990), COMMIT_1990, Field::from(2026u64)).unwrap();
    }

    #[test]
    fn minor_fails() {
        // 2026 - 2020 = 6 < 18 → the age assert must fail (commitment is valid)
        assert!(age_verification(identity(2020), COMMIT_2020, Field::from(2026u64)).is_err());
    }

    #[test]
    fn wrong_opening_fails() {
        // A commitment that doesn't match (birth_year, nonce) must be rejected.
        assert!(age_verification(identity(1990), COMMIT_2020, Field::from(2026u64)).is_err());
    }
}
