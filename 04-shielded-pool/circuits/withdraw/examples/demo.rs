//! Withdrawing the only note in the pool: proved as the verifier crate's self-test
//! vector (`proof.bin`, `public_inputs.bin`, as `xark prove` writes them). `just
//! build` runs it after `xark setup`.
use pool::Path;
use shielded_pool_withdraw::{Note, ShieldedPoolWithdrawInputs, Withdrawal};
use xark::Field;

fn main() {
    let note = Note {
        nullifier: Field::from(1u64),
        secret: Field::from(2u64),
    };
    let path = Path::first();
    let inputs = ShieldedPoolWithdrawInputs {
        note,
        path,
        withdrawal: Withdrawal::of(note, path, [1; 32].into(), [2; 32].into(), 0),
    };
    let out = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/target/xark/shielded_pool_withdraw"
    );
    let proof = xark::prover::circuit_at(out)
        .prove(inputs)
        .expect("the demo proves");
    std::fs::write(format!("{out}/proof.bin"), proof.proof).unwrap();
    std::fs::write(format!("{out}/public_inputs.bin"), proof.public_inputs).unwrap();
}
