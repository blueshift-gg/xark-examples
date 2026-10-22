//! A holder born 1990-01-01, checked on 2026-10-22: proved as the verifier crate's
//! self-test vector (`proof.bin`, `public_inputs.bin`, as `xark prove` writes them).
//! `just build` runs it after `xark setup`.
use age_verification::{AgeVerificationInputs, Identity, Policy};
use xark::Field;

fn main() {
    let id = Identity {
        born: "1990-01-01".into(),
        salt: Field::from_le_bytes(&[0x2a; 31]),
    };
    let inputs = AgeVerificationInputs {
        id,
        commitment: id.commitment(),
        policy: Policy::adult_on("2026-10-22"),
        holder: [7; 32].into(),
    };
    let out = concat!(env!("CARGO_MANIFEST_DIR"), "/target/xark/age_verification");
    let proof = xark::prover::circuit_at(out)
        .prove(inputs)
        .expect("the demo proves");
    std::fs::write(format!("{out}/proof.bin"), proof.proof).unwrap();
    std::fs::write(format!("{out}/public_inputs.bin"), proof.public_inputs).unwrap();
}
