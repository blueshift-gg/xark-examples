use token_claim_xark_verifier::{Proof, verify};

use crate::{instructions::public_inputs, state::Distributor};

/// The demo proof (`examples/demo.rs`: a list of one claim, 1000 for `[7; 32]`)
/// verifies against the public inputs the program assembles, for its claimant,
/// amount and root only.
#[test]
fn the_demo_proof_verifies_for_its_claim_only() {
    let proof = include_bytes!("../../circuit/token_claim-verifier/proof.solana.bin");
    let inputs = include_bytes!("../../circuit/token_claim-verifier/public_inputs.solana.bin");
    let proof = Proof::from_le_bytes(proof);
    let root: &[u8; 32] = inputs.first_chunk().unwrap();
    assert!(verify(&proof, &public_inputs(root, &[7; 32], 1_000)));
    assert!(!verify(&proof, &public_inputs(root, &[8; 32], 1_000)));
    assert!(!verify(&proof, &public_inputs(root, &[7; 32], 1_001)));
    assert!(!verify(&proof, &public_inputs(&[1; 32], &[7; 32], 1_000)));
}

/// The distributor's layout is the one the README and the clients read: `root` ‖
/// `mint` ‖ its bump ‖ the vault's, 66 bytes.
#[test]
fn the_distributor_is_66_bytes() {
    assert_eq!(Distributor::LEN, 66);
    assert_eq!(align_of::<Distributor>(), 1);
}
