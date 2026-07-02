// Prove age ≥ 18 against a public commitment to a birth year, revealing neither
// the year nor the nonce. Public: commitment, current_year (this order).
// Private: birth_year, nonce. (Trust model + walkthrough in the README.)

// Fixed-arity Poseidon2 compression. beta.22 exposes only the permutation
// publicly (xark's `Poseidon2Permutation` black-box); the client reproduces
// this exact construction.
fn hash2(a: Field, b: Field) -> Field {
    std::hash::poseidon2_permutation([a, b, 0, 0])[0]
}

fn main(
    birth_year: u32,
    nonce: Field,
    commitment: pub Field,
    current_year: pub u32,
) {
    // 1. Prove we know the opening of the published commitment.
    let recomputed = hash2(birth_year as Field, nonce);
    assert(recomputed == commitment);

    // 2. Prove adulthood. u32 subtraction underflows if birth_year > current_year,
    //    so we assert the ordering first, then bound the age.
    assert(current_year >= birth_year);
    let age = current_year - birth_year;
    assert(age >= 18);
}

// Helper: run `nargo test --show-output print_commitment` to compute the
// commitment for the birth_year/nonce below, then paste it into Prover.toml.
// Keep these two values in sync with Prover.toml.
#[test]
fn print_commitment() {
    let birth_year: Field = 1990;
    let nonce: Field = 0x1cf7e9a3b5d20486f1a9c8e7b6d5049382716f5e4d3c2b1a09f8e7d6c5b4a392;
    let commitment = hash2(birth_year, nonce);
    println(commitment);
}

#[test]
fn adult_passes() {
    // 2026 - 1990 = 36 >= 18
    main(1990, 0x1cf7e9a3b5d20486f1a9c8e7b6d5049382716f5e4d3c2b1a09f8e7d6c5b4a392, 0x27ade1149a33a134222bcad0bf82b938df5a8672c5ab5095fb0f392567057729, 2026);
}

#[test(should_fail)]
fn minor_fails() {
    // 2026 - 2020 = 6 < 18 → the age assert must fail (commitment matches [2020, nonce])
    main(2020, 0x1cf7e9a3b5d20486f1a9c8e7b6d5049382716f5e4d3c2b1a09f8e7d6c5b4a392, 0x234695133ea768cb71bdf1bfbfc2cd8ee4b44d7fb68e60a340ff0786b28832cc, 2026);
}
