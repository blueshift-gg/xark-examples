//! The first deposit into an empty pool: proved as the verifier crate's self-test
//! vector (`proof.bin`, `public_inputs.bin`, as `xark prove` writes them). `just
//! build` runs it after `xark setup`.
use pool::Path;
use shielded_pool_deposit::{Append, ShieldedPoolDepositInputs};
use xark::Field;

fn main() {
    let path = Path::first();
    let inputs = ShieldedPoolDepositInputs {
        path,
        append: Append::of(path, Field::from(7u64)),
    };
    let out = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/target/xark/shielded_pool_deposit"
    );
    let proof = xark::prover::circuit_at(out)
        .prove(inputs)
        .expect("the demo proves");
    std::fs::write(format!("{out}/proof.bin"), proof.proof).unwrap();
    std::fs::write(format!("{out}/public_inputs.bin"), proof.public_inputs).unwrap();
}
