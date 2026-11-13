//! A compliant holder checked on 2026-10-22 against the program's policy: proved as
//! the verifier crate's sample (`proof.bin`, `public_inputs.bin`, as `xark prove`
//! writes them), which `xark build` bundles into `clients/zkyc/rust` with its typed
//! self-test and keeps while it verifies. Run it once after a new key (`cargo run
//! --release -p zkyc --example demo`, then `xark build`). The program's tests verify
//! this proof against the public inputs the program assembles.
use xark::Field;
use xark::prelude::Country::*;
use zkyc::{Identity, Policy, ZkycInputs};

fn main() {
    let id = Identity {
        document: "A1938274".into(),
        born: "1990-01-01".into(),
        nationality: US.into(),
        net_worth: 1_000_000,
        expires: "2030-01-01".into(),
        salt: Field::from_le_bytes(&[0x13; 31]),
    };
    let inputs = ZkycInputs {
        id,
        commitment: id.commitment(),
        policy: Policy::default()
            .on("2026-10-22")
            .min_age(18)
            .worth_at_least(500_000)
            .nationalities([US, GB, FR, DE]),
        holder: [7; 32].into(),
    };
    let out = concat!(env!("CARGO_MANIFEST_DIR"), "/target/xark/zkyc");
    let proof = xark::prover::circuit_at(out)
        .prove(inputs)
        .expect("the demo proves");
    std::fs::write(format!("{out}/proof.bin"), proof.proof).unwrap();
    std::fs::write(format!("{out}/public_inputs.bin"), proof.public_inputs).unwrap();
}
