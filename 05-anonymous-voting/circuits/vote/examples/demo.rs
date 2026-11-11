//! A list of one voter, voting for option 1 of 3 in poll 7: proved as the verifier
//! crate's self-test vector (`proof.bin`, `public_inputs.bin`, as `xark prove`
//! writes them), which `xark build` bundles into `clients/vote/rust`. Run it after a
//! new `xark setup`: `cargo run --release -p vote --example demo`.
use vote::{Ballot, Identity, Path, VoteInputs};
use xark::Field;

fn main() {
    let identity = Identity {
        nullifier_key: Field::from(1u64),
        trapdoor: Field::from(2u64),
    };
    let path = Path::first();
    let inputs = VoteInputs {
        identity,
        path,
        ballot: Ballot::of(identity, path, Field::from(7u64), 3, 1),
    };
    let out = concat!(env!("CARGO_MANIFEST_DIR"), "/target/xark/vote");
    let proof = xark::prover::circuit_at(out)
        .prove(inputs)
        .expect("the demo proves");
    std::fs::write(format!("{out}/proof.bin"), proof.proof).unwrap();
    std::fs::write(format!("{out}/public_inputs.bin"), proof.public_inputs).unwrap();
}
