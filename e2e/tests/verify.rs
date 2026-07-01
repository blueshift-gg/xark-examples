//! End-to-end verification of the example programs inside an in-process Solana
//! VM (LiteSVM). Each test loads the compiled `.so`, submits the real
//! `instruction_data.bin` that `xark export` produced, and asserts:
//!   1. the valid proof verifies on-chain (through the native alt_bn128 syscalls), and
//!   2. a tampered proof is rejected.
//!
//! Prerequisites (see ../RUNBOOK.md): each example built via `just prove`,
//! `just export`, and `cargo build-sbf`. If artifacts are missing the test
//! panics with the missing path.
use std::path::PathBuf;

use litesvm::LiteSVM;
use solana_instruction::Instruction;
use solana_keypair::Keypair;
use solana_message::Message;
use solana_signer::Signer;
use solana_transaction::Transaction;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("e2e has a parent dir")
        .to_path_buf()
}

fn read(rel: &str) -> Vec<u8> {
    let path: PathBuf = repo_root().join(rel);
    std::fs::read(&path).unwrap_or_else(|e| panic!("missing artifact {}: {e}", path.display()))
}

/// Load `so_rel` into LiteSVM and submit `ix_data`. Returns whether the tx succeeded.
fn submit(so: &[u8], ix_data: Vec<u8>) -> bool {
    let mut svm = LiteSVM::new();
    let program_id = Keypair::new().pubkey();
    svm.add_program(program_id, so).expect("load program");
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 1_000_000_000).expect("airdrop");

    let ix = Instruction {
        program_id,
        accounts: vec![],
        data: ix_data,
    };
    let msg = Message::new(&[ix], Some(&payer.pubkey()));
    let tx = Transaction::new(&[&payer], msg, svm.latest_blockhash());
    svm.send_transaction(tx).is_ok()
}

fn assert_verifies(so_rel: &str, ix_rel: &str) {
    let so = read(so_rel);
    let data = read(ix_rel);

    assert!(
        submit(&so, data.clone()),
        "valid proof should verify on-chain ({so_rel})",
    );

    // Flip a byte in the proof: it must no longer verify.
    let mut tampered = data.clone();
    tampered[0] ^= 0xff;
    assert!(
        !submit(&so, tampered),
        "tampered proof must be rejected ({so_rel})",
    );
}

#[test]
fn over_9000_verifies_on_chain() {
    assert_verifies(
        "01-over-9000/program/target/deploy/over_9000_program.so",
        "01-over-9000/circuit/target/over_9000-xark-verifier/instruction_data.bin",
    );
}

#[test]
fn age_verification_verifies_on_chain() {
    assert_verifies(
        "02-age-verification/program/target/deploy/age_verification_program.so",
        "02-age-verification/circuit/target/age_verification-xark-verifier/instruction_data.bin",
    );
}
