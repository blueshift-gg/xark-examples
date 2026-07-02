//! Full shielded-pool flow in an in-process Solana VM (LiteSVM):
//! initialize -> deposit -> withdraw, plus the program's rejection paths.
//! All field bytes come from the exported `*.solana.bin` artifacts so the
//! on-chain public inputs match the proofs exactly. Requires the pool built per
//! ../RUNBOOK.md.
use std::path::PathBuf;
use std::str::FromStr;

use litesvm::LiteSVM;
use litesvm::types::TransactionResult;
use sha2::{Digest, Sha256};
use solana_address::Address;
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_message::Message;
use solana_signer::Signer;
use solana_transaction::Transaction;

const PROGRAM_ID: &str = "Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS";
const DENOMINATION: u64 = 1_000_000_000; // 1 SOL

// Test keypairs bound into the withdraw proof (see circuits/withdraw/Prover.toml).
const RELAYER: [u8; 64] = [
    222, 27, 248, 241, 243, 155, 183, 138, 72, 86, 34, 113, 80, 238, 215, 7, 219, 136, 30, 144,
    122, 237, 63, 116, 254, 188, 128, 113, 105, 153, 172, 56, 91, 240, 156, 105, 6, 194, 63, 197,
    56, 125, 221, 53, 2, 248, 135, 176, 89, 78, 14, 41, 204, 168, 171, 198, 70, 69, 81, 57, 44,
    158, 77, 232,
];
const RECIPIENT: [u8; 64] = [
    212, 15, 49, 134, 72, 242, 239, 139, 44, 143, 167, 93, 16, 39, 229, 29, 13, 215, 136, 22, 134,
    121, 177, 47, 4, 83, 95, 86, 234, 242, 171, 109, 137, 187, 244, 226, 218, 76, 53, 133, 74, 6,
    206, 253, 228, 242, 220, 84, 148, 212, 171, 202, 211, 57, 164, 158, 9, 139, 238, 207, 95, 214,
    211, 43,
];

/// Anchor instruction discriminator = sha256("global:<name>")[..8].
fn disc(name: &str) -> [u8; 8] {
    Sha256::digest(format!("global:{name}").as_bytes())[..8].try_into().unwrap()
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}
fn read(rel: &str) -> Vec<u8> {
    let p = repo_root().join(rel);
    std::fs::read(&p).unwrap_or_else(|e| panic!("missing {}: {e}", p.display()))
}
fn chunk32(bytes: &[u8], i: usize) -> [u8; 32] {
    bytes[i * 32..i * 32 + 32].try_into().unwrap()
}
fn borsh_bytes(v: &[u8]) -> Vec<u8> {
    let mut out = (v.len() as u32).to_le_bytes().to_vec();
    out.extend_from_slice(v);
    out
}
fn send(svm: &mut LiteSVM, ix: Instruction, signer: &Keypair) -> TransactionResult {
    let msg = Message::new(&[ix], Some(&signer.pubkey()));
    let tx = Transaction::new(&[signer], msg, svm.latest_blockhash());
    svm.send_transaction(tx)
}

const DEPOSIT_DIR: &str =
    "03-shielded-pool/circuits/deposit/target/shielded_pool_deposit-xark-verifier";
const WITHDRAW_DIR: &str =
    "03-shielded-pool/circuits/withdraw/target/shielded_pool_withdraw-xark-verifier";

/// A pool that's been initialized and has one deposit, ready to withdraw.
struct Pool {
    svm: LiteSVM,
    program_id: Address,
    system: Address,
    pool: Address,
    relayer: Keypair,
    recipient: Keypair,
    withdraw_proof: Vec<u8>,
    w_root: [u8; 32],
    nullifier_hash: [u8; 32],
}

fn setup() -> Pool {
    let program_id = Address::from_str(PROGRAM_ID).unwrap();
    let system = Address::from_str("11111111111111111111111111111111").unwrap();

    // Deposit public inputs: [old_root, new_root, commitment, index].
    let dpi = read(&format!("{DEPOSIT_DIR}/public_inputs.solana.bin"));
    assert_eq!(dpi.len(), 4 * 32, "deposit must expose 4 public inputs");
    let new_root = chunk32(&dpi, 1);
    let commitment = chunk32(&dpi, 2);
    let deposit_proof = read(&format!("{DEPOSIT_DIR}/proof.solana.bin"));

    // Withdraw public inputs: [root, nullifier_hash, r_hi, r_lo, l_hi, l_lo, fee].
    let wpi = read(&format!("{WITHDRAW_DIR}/public_inputs.solana.bin"));
    assert_eq!(wpi.len(), 7 * 32, "withdraw must expose 7 public inputs");
    let w_root = chunk32(&wpi, 0);
    let nullifier_hash = chunk32(&wpi, 1);
    let withdraw_proof = read(&format!("{WITHDRAW_DIR}/proof.solana.bin"));
    assert_eq!(w_root, new_root, "withdraw root must equal the deposit's new root");

    let mut svm = LiteSVM::new();
    svm.add_program(program_id, &read("03-shielded-pool/program/target/deploy/shielded_pool.so"))
        .unwrap();

    let authority = Keypair::new();
    let depositor = Keypair::new();
    let relayer = Keypair::new_from_array(RELAYER[..32].try_into().unwrap());
    let recipient = Keypair::new_from_array(RECIPIENT[..32].try_into().unwrap());
    for kp in [&authority, &depositor, &relayer] {
        svm.airdrop(&kp.pubkey(), 3 * DENOMINATION).unwrap();
    }

    let (pool, _) = Address::find_program_address(&[b"pool"], &program_id);

    // initialize(denomination) — the empty-tree root is fixed on-chain.
    let mut data = disc("initialize").to_vec();
    data.extend_from_slice(&DENOMINATION.to_le_bytes());
    send(
        &mut svm,
        Instruction {
            program_id,
            accounts: vec![
                AccountMeta::new(pool, false),
                AccountMeta::new(authority.pubkey(), true),
                AccountMeta::new_readonly(system, false),
            ],
            data,
        },
        &authority,
    )
    .expect("initialize");

    // deposit(commitment, new_root, proof)
    let mut data = disc("deposit").to_vec();
    data.extend_from_slice(&commitment);
    data.extend_from_slice(&new_root);
    data.extend_from_slice(&borsh_bytes(&deposit_proof));
    send(
        &mut svm,
        Instruction {
            program_id,
            accounts: vec![
                AccountMeta::new(pool, false),
                AccountMeta::new(depositor.pubkey(), true),
                AccountMeta::new_readonly(system, false),
            ],
            data,
        },
        &depositor,
    )
    .expect("deposit");

    Pool { svm, program_id, system, pool, relayer, recipient, withdraw_proof, w_root, nullifier_hash }
}

/// Build a withdraw instruction, allowing each field to be overridden for the
/// negative tests.
fn withdraw_ix(
    p: &Pool,
    proof: &[u8],
    root: [u8; 32],
    nullifier_hash: [u8; 32],
    fee: u64,
    recipient: Address,
) -> Instruction {
    let (nullifier, _) =
        Address::find_program_address(&[b"nullifier", &nullifier_hash], &p.program_id);
    let mut data = disc("withdraw").to_vec();
    data.extend_from_slice(&borsh_bytes(proof));
    data.extend_from_slice(&root);
    data.extend_from_slice(&nullifier_hash);
    data.extend_from_slice(&fee.to_le_bytes());
    Instruction {
        program_id: p.program_id,
        accounts: vec![
            AccountMeta::new(p.pool, false),
            AccountMeta::new(nullifier, false),
            AccountMeta::new(recipient, false),
            AccountMeta::new(p.relayer.pubkey(), true),
            AccountMeta::new_readonly(p.system, false),
        ],
        data,
    }
}

#[test]
fn shielded_pool_full_flow() {
    let mut p = setup();
    let before = p.svm.get_balance(&p.recipient.pubkey()).unwrap_or(0);

    let ix = withdraw_ix(&p, &p.withdraw_proof, p.w_root, p.nullifier_hash, 0, p.recipient.pubkey());
    let relayer = p.relayer.insecure_clone();
    send(&mut p.svm, ix, &relayer).expect("withdraw");

    let after = p.svm.get_balance(&p.recipient.pubkey()).unwrap_or(0);
    assert_eq!(after - before, DENOMINATION, "recipient receives the full denomination");

    // Double-spend: same nullifier must fail.
    let ix = withdraw_ix(&p, &p.withdraw_proof, p.w_root, p.nullifier_hash, 0, p.recipient.pubkey());
    assert!(send(&mut p.svm, ix, &relayer).is_err(), "double-spend must be rejected");
}

#[test]
fn withdraw_rejects_unknown_root() {
    let mut p = setup();
    let bad_root = [9u8; 32];
    let ix = withdraw_ix(&p, &p.withdraw_proof, bad_root, p.nullifier_hash, 0, p.recipient.pubkey());
    let relayer = p.relayer.insecure_clone();
    assert!(send(&mut p.svm, ix, &relayer).is_err(), "unknown root must be rejected");
}

#[test]
fn withdraw_rejects_fee_above_denomination() {
    let mut p = setup();
    let ix = withdraw_ix(
        &p,
        &p.withdraw_proof,
        p.w_root,
        p.nullifier_hash,
        DENOMINATION + 1,
        p.recipient.pubkey(),
    );
    let relayer = p.relayer.insecure_clone();
    assert!(send(&mut p.svm, ix, &relayer).is_err(), "fee > denomination must be rejected");
}

#[test]
fn withdraw_rejects_bad_proof_len() {
    let mut p = setup();
    let short = p.withdraw_proof[..255].to_vec();
    let ix = withdraw_ix(&p, &short, p.w_root, p.nullifier_hash, 0, p.recipient.pubkey());
    let relayer = p.relayer.insecure_clone();
    assert!(send(&mut p.svm, ix, &relayer).is_err(), "wrong proof length must be rejected");
}

#[test]
fn withdraw_rejects_tampered_proof() {
    let mut p = setup();
    let mut proof = p.withdraw_proof.clone();
    proof[0] ^= 0xff;
    let ix = withdraw_ix(&p, &proof, p.w_root, p.nullifier_hash, 0, p.recipient.pubkey());
    let relayer = p.relayer.insecure_clone();
    assert!(send(&mut p.svm, ix, &relayer).is_err(), "tampered proof must be rejected");
}

#[test]
fn withdraw_rejects_wrong_recipient() {
    let mut p = setup();
    // A recipient not bound into the proof → derived public inputs won't match.
    let wrong = Keypair::new().pubkey();
    let ix = withdraw_ix(&p, &p.withdraw_proof, p.w_root, p.nullifier_hash, 0, wrong);
    let relayer = p.relayer.insecure_clone();
    assert!(send(&mut p.svm, ix, &relayer).is_err(), "wrong recipient must be rejected");
}
