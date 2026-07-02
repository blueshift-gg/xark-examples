//! Shared helpers for the e2e integration tests (loaded via `mod common;`).
#![allow(dead_code)] // each test binary uses a subset

use std::path::PathBuf;

use litesvm::LiteSVM;
use litesvm::types::TransactionResult;
use sha2::{Digest, Sha256};
use solana_instruction::Instruction;
use solana_keypair::Keypair;
use solana_message::Message;
use solana_signer::Signer;
use solana_transaction::Transaction;

/// Repo root (the parent of the `e2e/` crate).
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

/// Read a repo-relative file, panicking with the path if missing.
pub fn read(rel: &str) -> Vec<u8> {
    let p = repo_root().join(rel);
    std::fs::read(&p).unwrap_or_else(|e| panic!("missing {}: {e}", p.display()))
}

/// The i-th 32-byte little-endian field element in a public-inputs blob.
pub fn chunk32(b: &[u8], i: usize) -> [u8; 32] {
    b[i * 32..i * 32 + 32].try_into().unwrap()
}

/// Low 8 bytes of the i-th field element, as u64 (little-endian).
pub fn u64_at(b: &[u8], i: usize) -> u64 {
    u64::from_le_bytes(b[i * 32..i * 32 + 8].try_into().unwrap())
}

/// Borsh encoding of a byte vector: u32 length prefix (LE) + bytes.
pub fn borsh_bytes(v: &[u8]) -> Vec<u8> {
    let mut out = (v.len() as u32).to_le_bytes().to_vec();
    out.extend_from_slice(v);
    out
}

/// Anchor instruction discriminator = sha256("global:<name>")[..8].
pub fn disc(name: &str) -> [u8; 8] {
    Sha256::digest(format!("global:{name}").as_bytes())[..8].try_into().unwrap()
}

/// Sign + send a single-instruction transaction.
pub fn send(svm: &mut LiteSVM, ix: Instruction, signer: &Keypair) -> TransactionResult {
    let msg = Message::new(&[ix], Some(&signer.pubkey()));
    svm.send_transaction(Transaction::new(&[signer], msg, svm.latest_blockhash()))
}
