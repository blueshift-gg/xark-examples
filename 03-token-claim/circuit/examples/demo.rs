//! A list of one claim, 1000 tokens for the wallet `[7; 32]`, claimed: proved as the
//! verifier crate's self-test vector (`proof.bin`, `public_inputs.bin`, as `xark
//! prove` writes them). `just build` runs it after `xark setup`.
use token_claim::{Claim, Path, TokenClaimInputs};

fn main() {
    let claim = Claim {
        claimant: [7; 32].into(),
        amount: 1_000,
    };
    let path = Path::first();
    let inputs = TokenClaimInputs {
        path,
        root: path.next(claim.leaf()).list_root(),
        claim,
    };
    let out = concat!(env!("CARGO_MANIFEST_DIR"), "/target/xark/token_claim");
    let proof = xark::prover::circuit_at(out)
        .prove(inputs)
        .expect("the demo proves");
    std::fs::write(format!("{out}/proof.bin"), proof.proof).unwrap();
    std::fs::write(format!("{out}/public_inputs.bin"), proof.public_inputs).unwrap();
}
