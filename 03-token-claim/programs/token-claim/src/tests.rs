use token_claim_verifier::{Claim, Proof, PublicInputs, verify};

use crate::state::Distributor;

/// The demo proof (`circuits/token-claim/examples/demo.rs`: a list of one claim,
/// 1000 for `[7; 32]`) verifies for its claimant, amount and root only.
#[test]
fn the_demo_proof_verifies_for_its_claim_only() {
    let proof = include_bytes!("../../../clients/token-claim/rust/proof.solana.bin");
    let fields = include_bytes!("../../../clients/token-claim/rust/public_inputs.solana.bin");
    let proof = Proof::from_le_bytes(proof);
    let root = *fields.first_chunk().unwrap();
    let inputs = |claimant, amount| PublicInputs {
        root,
        claim: Claim { claimant, amount },
    };
    assert!(verify(&proof, inputs([7; 32], 1_000)));
    assert!(!verify(&proof, inputs([8; 32], 1_000)));
    assert!(!verify(&proof, inputs([7; 32], 1_001)));
    assert!(!verify(
        &proof,
        PublicInputs {
            root: [1; 32],
            ..inputs([7; 32], 1_000)
        }
    ));
}

/// The distributor's layout is the one the README and the clients read: `root` ‖
/// `mint` ‖ its bump ‖ the vault's, 66 bytes.
#[test]
fn the_distributor_is_66_bytes() {
    assert_eq!(Distributor::LEN, 66);
    assert_eq!(align_of::<Distributor>(), 1);
}
