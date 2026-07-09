//! Prove age ≥ 18 against a public commitment to a birth year, revealing
//! neither the year nor the nonce. Public: commitment, current_year (this
//! order — the program packs instruction data to match). Private: birth_year,
//! nonce. (Trust model + walkthrough in the README.)
#![cfg_attr(not(test), no_std)]

use xark::prelude::*;
use xark_poseidon2::hash2;

pub fn circuit(
    birth_year: Private<Field>,
    nonce: Private<Field>,
    commitment: Public<Field>,
    current_year: Public<Field>,
) {
    // 1. Prove we know the opening of the published commitment. `hash2` is the
    //    same Poseidon2 gadget the client hashes with — params match forever.
    assert_eq(hash2(birth_year, nonce), commitment);

    // 2. Prove adulthood. The explicit `::<32>` width range-checks both years
    //    to 32 bits, so the subtraction below cannot wrap the field; `18u32`
    //    then bounds the age the same way.
    assert(current_year.ge::<32>(birth_year));
    let age = current_year - birth_year;
    assert(age >= 18u32);
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use xark_ir::VarId;

    /// Prove with decimal-string inputs (commitments don't fit the i128-typed
    /// `#[circuit]` test struct). Loads the artifacts `xark build` wrote.
    fn prove(inputs: &[(&str, &str)]) -> bool {
        let dir = std::path::Path::new("target/xark/age_verification");
        let read = |f: &str| {
            std::fs::read_to_string(dir.join(f))
                .unwrap_or_else(|_| panic!("run `xark build` first (missing {f})"))
        };
        let prim = xark_ir::primitive::from_json(&read("circuit.json")).unwrap();
        let r1cs = xark_ir::json::from_json(&read("r1cs.json")).unwrap();
        let ids: BTreeMap<&str, VarId> =
            prim.vars.iter().map(|v| (v.name.as_str(), v.id)).collect();
        let inputs: BTreeMap<VarId, String> = inputs
            .iter()
            .map(|(name, val)| (ids[name], val.to_string()))
            .collect();
        xark_prover::prove_and_verify(&r1cs, &prim, &inputs).unwrap_or(false)
    }

    // Poseidon2(birth_year, nonce) for the two test years — computed with the
    // repo's native mirror of the gadget (`cargo run --bin commit`, e2e crate).
    const NONCE: &str =
        "13102783505849749806854498441424493672889223651541975554449217606361432761234";
    const COMMIT_1990: &str =
        "16270599866198939632822375879477475677301143667419388581635073777919968987292";
    const COMMIT_2020: &str =
        "13573765523911323905052498488738541803332695318708852347910960062058857884122";

    #[test]
    fn adult_passes() {
        // 2026 - 1990 = 36 ≥ 18
        assert!(prove(&[
            ("birth_year", "1990"),
            ("nonce", NONCE),
            ("commitment", COMMIT_1990),
            ("current_year", "2026"),
        ]));
    }

    #[test]
    fn minor_fails() {
        // 2026 - 2020 = 6 < 18 → the age assert must fail (commitment is valid)
        assert!(!prove(&[
            ("birth_year", "2020"),
            ("nonce", NONCE),
            ("commitment", COMMIT_2020),
            ("current_year", "2026"),
        ]));
    }

    #[test]
    fn wrong_opening_fails() {
        // A commitment that doesn't match (birth_year, nonce) must be rejected.
        assert!(!prove(&[
            ("birth_year", "1990"),
            ("nonce", NONCE),
            ("commitment", COMMIT_2020),
            ("current_year", "2026"),
        ]));
    }
}
