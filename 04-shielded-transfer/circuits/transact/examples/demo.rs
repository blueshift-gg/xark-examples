//! A deposit of 150 into an empty pool: proved as the verifier crate's self-test
//! vector (`proof.bin`, `public_inputs.bin`, as `xark prove` writes them). `just
//! build` runs it after `xark setup`.
use pool::Path;
use shielded_transact::{Flow, Key, Note, ShieldedTransactInputs, Spend, Transaction};
use xark::Field;

fn main() {
    let key = Key {
        secret: Field::from(42u64),
    };
    let asset = [7; 32].into();
    let note = |value, rho: u64| Note {
        value,
        owner: key.address(Field::from(1u64)),
        rho: Field::from(rho),
        rseed: Field::from(rho + 1),
    };
    let dummy = |rho| Spend {
        note: note(0, rho),
        diversifier: Field::from(1u64),
        path: Path::first(),
        real: false,
    };
    let spends = [dummy(100), dummy(101)];
    let outputs = [note(150, 1), note(0, 2)];
    let flow = Flow {
        vpub_in: 150,
        vpub_out: 0,
        fee: 0,
        recipient: [9; 32].into(),
        relayer: [8; 32].into(),
    };
    let memos = [[1; 32].into(), [2; 32].into()];
    let tip = Path::first();
    let inputs = ShieldedTransactInputs {
        key,
        spends,
        outputs,
        tip,
        tx: Transaction::of(key, spends, outputs, tip, asset, flow, memos),
    };
    let out = concat!(env!("CARGO_MANIFEST_DIR"), "/target/xark/shielded_transact");
    let proof = xark::prover::circuit_at(out)
        .prove(inputs)
        .expect("the demo proves");
    std::fs::write(format!("{out}/proof.bin"), proof.proof).unwrap();
    std::fs::write(format!("{out}/public_inputs.bin"), proof.public_inputs).unwrap();
}
