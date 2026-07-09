//! Full shielded-pool flow in an in-process Solana VM (LiteSVM):
//! initialize -> deposit -> withdraw, plus the program's rejection paths.
//! All field bytes come from the exported `*.solana.bin` artifacts so the
//! on-chain public inputs match the proofs exactly. Requires the pool built per
//! ../RUNBOOK.md.
mod common;
use common::{borsh_bytes, chunk32, disc, read, send};

use std::str::FromStr;

use litesvm::LiteSVM;
use solana_address::Address;
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_signer::Signer;

const PROGRAM_ID: &str = "Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS";
const DENOMINATION: u64 = 1_000_000_000; // 1 SOL

// Test keypairs bound into the withdraw proof — shared with the witness
// generator (`pool-witness`) via the e2e lib, so prover and chain always agree.
use xark_examples_e2e::fixtures::{RECIPIENT, RELAYER};

const DEPOSIT_DIR: &str =
    "03-shielded-pool/circuits/deposit/target/xark/shielded_pool_deposit/verifier";
const WITHDRAW_DIR: &str =
    "03-shielded-pool/circuits/withdraw/target/xark/shielded_pool_withdraw/verifier";

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
    assert_eq!(
        w_root, new_root,
        "withdraw root must equal the deposit's new root"
    );

    let mut svm = LiteSVM::new().with_mainnet_features();
    svm.add_program(
        program_id,
        &read("03-shielded-pool/program/target/deploy/shielded_pool.so"),
    )
    .unwrap();

    let authority = Keypair::new();
    let depositor = Keypair::new();
    let relayer = Keypair::new_from_array(RELAYER[..32].try_into().unwrap());
    let recipient = Keypair::new_from_array(RECIPIENT[..32].try_into().unwrap());
    for kp in [&authority, &depositor, &relayer] {
        svm.airdrop(&kp.pubkey(), 3 * DENOMINATION).unwrap();
    }

    let (pool, _) =
        Address::find_program_address(&[b"pool", &DENOMINATION.to_le_bytes()], &program_id);

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

    Pool {
        svm,
        program_id,
        system,
        pool,
        relayer,
        recipient,
        withdraw_proof,
        w_root,
        nullifier_hash,
    }
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
    let (nullifier, _) = Address::find_program_address(
        &[b"nullifier", &DENOMINATION.to_le_bytes(), &nullifier_hash],
        &p.program_id,
    );
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

    let ix = withdraw_ix(
        &p,
        &p.withdraw_proof,
        p.w_root,
        p.nullifier_hash,
        0,
        p.recipient.pubkey(),
    );
    let relayer = p.relayer.insecure_clone();
    send(&mut p.svm, ix, &relayer).expect("withdraw");

    let after = p.svm.get_balance(&p.recipient.pubkey()).unwrap_or(0);
    assert_eq!(
        after - before,
        DENOMINATION,
        "recipient receives the full denomination"
    );

    // Double-spend: same nullifier must fail.
    let ix = withdraw_ix(
        &p,
        &p.withdraw_proof,
        p.w_root,
        p.nullifier_hash,
        0,
        p.recipient.pubkey(),
    );
    assert!(
        send(&mut p.svm, ix, &relayer).is_err(),
        "double-spend must be rejected"
    );
}

#[test]
fn withdraw_rejects_unknown_root() {
    let mut p = setup();
    let bad_root = [9u8; 32];
    let ix = withdraw_ix(
        &p,
        &p.withdraw_proof,
        bad_root,
        p.nullifier_hash,
        0,
        p.recipient.pubkey(),
    );
    let relayer = p.relayer.insecure_clone();
    assert!(
        send(&mut p.svm, ix, &relayer).is_err(),
        "unknown root must be rejected"
    );
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
    assert!(
        send(&mut p.svm, ix, &relayer).is_err(),
        "fee > denomination must be rejected"
    );
}

#[test]
fn withdraw_rejects_bad_proof_len() {
    let mut p = setup();
    let short = p.withdraw_proof[..255].to_vec();
    let ix = withdraw_ix(
        &p,
        &short,
        p.w_root,
        p.nullifier_hash,
        0,
        p.recipient.pubkey(),
    );
    let relayer = p.relayer.insecure_clone();
    assert!(
        send(&mut p.svm, ix, &relayer).is_err(),
        "wrong proof length must be rejected"
    );
}

#[test]
fn withdraw_rejects_tampered_proof() {
    let mut p = setup();
    let mut proof = p.withdraw_proof.clone();
    proof[0] ^= 0xff;
    let ix = withdraw_ix(
        &p,
        &proof,
        p.w_root,
        p.nullifier_hash,
        0,
        p.recipient.pubkey(),
    );
    let relayer = p.relayer.insecure_clone();
    assert!(
        send(&mut p.svm, ix, &relayer).is_err(),
        "tampered proof must be rejected"
    );
}

#[test]
fn withdraw_rejects_wrong_recipient() {
    let mut p = setup();
    // A different recipient changes the proof-bound public inputs.
    let wrong = Keypair::new().pubkey();
    let ix = withdraw_ix(&p, &p.withdraw_proof, p.w_root, p.nullifier_hash, 0, wrong);
    let relayer = p.relayer.insecure_clone();
    assert!(
        send(&mut p.svm, ix, &relayer).is_err(),
        "wrong recipient must be rejected"
    );
}

#[test]
fn two_denominations_coexist() {
    let program_id = Address::from_str(PROGRAM_ID).unwrap();
    let system = Address::from_str("11111111111111111111111111111111").unwrap();
    let mut svm = LiteSVM::new().with_mainnet_features();
    svm.add_program(
        program_id,
        &read("03-shielded-pool/program/target/deploy/shielded_pool.so"),
    )
    .unwrap();
    let authority = Keypair::new();
    svm.airdrop(&authority.pubkey(), 100 * DENOMINATION)
        .unwrap();

    let init = |svm: &mut LiteSVM, denom: u64| -> (Address, bool) {
        let (pool, _) =
            Address::find_program_address(&[b"pool", &denom.to_le_bytes()], &program_id);
        let mut data = disc("initialize").to_vec();
        data.extend_from_slice(&denom.to_le_bytes());
        let ix = Instruction {
            program_id,
            accounts: vec![
                AccountMeta::new(pool, false),
                AccountMeta::new(authority.pubkey(), true),
                AccountMeta::new_readonly(system, false),
            ],
            data,
        };
        (pool, send(svm, ix, &authority).is_ok())
    };

    let (pool_a, ok_a) = init(&mut svm, 1_000_000_000);
    let (pool_b, ok_b) = init(&mut svm, 10_000_000_000);
    assert!(ok_a && ok_b, "both denominations initialize");
    assert_ne!(pool_a, pool_b, "each denomination gets a distinct pool PDA");
    assert!(
        svm.get_account(&pool_a).is_some() && svm.get_account(&pool_b).is_some(),
        "both pools exist independently",
    );
}
