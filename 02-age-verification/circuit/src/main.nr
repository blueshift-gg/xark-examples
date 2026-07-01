// Age verification — prove you're 18+ without revealing your birthday.
//
// The trust model (honest about it): an ISSUER commits, once, to your birth
// year and publishes only the commitment:
//
//     commitment = Poseidon2::hash([birth_year, nonce])
//
// `nonce` is a random blinding value so the commitment can't be brute-forced
// (there are only ~120 plausible birth years). Later, you — or anyone holding
// the opening — can prove "age >= 18" against that public commitment as many
// times as you like, revealing neither the birth year nor the nonce.
//
// Public inputs:  commitment, current_year   (declared in this order)
// Private inputs: birth_year, nonce
//
// NOTE: we use the canonical `std::hash::poseidon2::Poseidon2::hash`, which
// lowers to the `Poseidon2Permutation` black-box xark supports. If a future
// nargo changes that lowering, swap in a permutation-based compression (see
// ../README.md → "Poseidon compatibility").
use std::hash::poseidon2::Poseidon2;

fn main(
    birth_year: u32,
    nonce: Field,
    commitment: pub Field,
    current_year: pub u32,
) {
    // 1. Prove we know the opening of the published commitment.
    let recomputed = Poseidon2::hash([birth_year as Field, nonce], 2);
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
    let nonce: Field = 424242424242;
    let commitment = Poseidon2::hash([birth_year, nonce], 2);
    println(commitment);
}

#[test]
fn adult_passes() {
    // 2026 - 1990 = 36 >= 18. Uses the commitment printed by `print_commitment`.
    // (Replace the commitment literal after computing it locally.)
    // main(1990, 424242424242, <commitment>, 2026);
}

#[test(should_fail)]
fn minor_fails() {
    // 2026 - 2020 = 6 < 18  → must fail. (commitment must match [2020, nonce].)
    // main(2020, 424242424242, <commitment_for_2020>, 2026);
}
