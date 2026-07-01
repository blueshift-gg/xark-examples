//! Full shielded-pool flow in an in-process Solana VM (LiteSVM):
//! initialize -> deposit (with the deposit proof) -> withdraw (with the withdraw
//! proof) to a fresh recipient. Asserts the recipient is paid and that a second
//! withdraw with the same nullifier fails (double-spend protection).
//!
//! All field bytes are read straight from the exported `*.solana.bin` artifacts
//! so the on-chain public inputs match the proofs exactly. Requires the pool
//! built per ../RUNBOOK.md (both circuits proved+exported, program built).
use std::path::PathBuf;
use std::str::FromStr;

use litesvm::LiteSVM;
use solana_address::Address;
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_message::Message;
use solana_signer::Signer;
use solana_transaction::Transaction;

const PROGRAM_ID: &str = "Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS";
const DENOMINATION: u64 = 1_000_000_000; // 1 SOL

// Anchor discriminators = sha256("global:<name>")[..8].
const DISC_INITIALIZE: [u8; 8] = [175, 175, 109, 31, 13, 152, 155, 237];
const DISC_DEPOSIT: [u8; 8] = [242, 35, 198, 137, 82, 225, 242, 182];
const DISC_WITHDRAW: [u8; 8] = [183, 18, 70, 156, 148, 109, 161, 34];

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

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}
fn read(rel: &str) -> Vec<u8> {
    let p = root().join(rel);
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

#[test]
fn shielded_pool_full_flow() {
    let program_id = Address::from_str(PROGRAM_ID).unwrap();
    let system = Address::from_str("11111111111111111111111111111111").unwrap();

    let deposit_dir = "03-shielded-pool/circuits/deposit/target/shielded_pool_deposit-xark-verifier";
    let withdraw_dir =
        "03-shielded-pool/circuits/withdraw/target/shielded_pool_withdraw-xark-verifier";

    // Deposit public inputs: [old_root, new_root, commitment, index] (LE 32B each).
    let dpi = read(&format!("{deposit_dir}/public_inputs.solana.bin"));
    let empty_root = chunk32(&dpi, 0); // == old_root of the first deposit
    let new_root = chunk32(&dpi, 1);
    let commitment = chunk32(&dpi, 2);
    let deposit_proof = read(&format!("{deposit_dir}/proof.solana.bin"));

    // Withdraw public inputs: [root, nullifier_hash, r_hi, r_lo, l_hi, l_lo, fee].
    let wpi = read(&format!("{withdraw_dir}/public_inputs.solana.bin"));
    let w_root = chunk32(&wpi, 0);
    let nullifier_hash = chunk32(&wpi, 1);
    let withdraw_proof = read(&format!("{withdraw_dir}/proof.solana.bin"));
    assert_eq!(w_root, new_root, "withdraw root must equal the deposit's new root");

    let mut svm = LiteSVM::new();
    svm.add_program(program_id, &read("03-shielded-pool/program/target/deploy/shielded_pool.so"))
        .unwrap();

    let authority = Keypair::new();
    let depositor = Keypair::new();
    // The keypair's 32-byte seed is the first half of the 64-byte secret key.
    let relayer = Keypair::new_from_array(RELAYER[..32].try_into().unwrap());
    let recipient = Keypair::new_from_array(RECIPIENT[..32].try_into().unwrap());
    for kp in [&authority, &depositor, &relayer] {
        svm.airdrop(&kp.pubkey(), 3 * DENOMINATION).unwrap();
    }

    let (pool, _) = Address::find_program_address(&[b"pool"], &program_id);
    let (nullifier, _) =
        Address::find_program_address(&[b"nullifier", &nullifier_hash], &program_id);

    let send = |svm: &mut LiteSVM, ix: Instruction, signers: &[&Keypair]| {
        let msg = Message::new(&[ix], Some(&signers[0].pubkey()));
        let tx = Transaction::new(signers, msg, svm.latest_blockhash());
        svm.send_transaction(tx)
    };

    // 1. initialize(denomination, empty_root)
    let mut data = DISC_INITIALIZE.to_vec();
    data.extend_from_slice(&DENOMINATION.to_le_bytes());
    data.extend_from_slice(&empty_root);
    let ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(pool, false),
            AccountMeta::new(authority.pubkey(), true),
            AccountMeta::new_readonly(system, false),
        ],
        data,
    };
    send(&mut svm, ix, &[&authority]).expect("initialize");

    // 2. deposit(commitment, new_root, proof)
    let mut data = DISC_DEPOSIT.to_vec();
    data.extend_from_slice(&commitment);
    data.extend_from_slice(&new_root);
    data.extend_from_slice(&borsh_bytes(&deposit_proof));
    let ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(pool, false),
            AccountMeta::new(depositor.pubkey(), true),
            AccountMeta::new_readonly(system, false),
        ],
        data,
    };
    send(&mut svm, ix, &[&depositor]).expect("deposit");

    // 3. withdraw(proof, root, nullifier_hash, fee=0) → pays recipient
    let recipient_before = svm.get_balance(&recipient.pubkey()).unwrap_or(0);
    let mut data = DISC_WITHDRAW.to_vec();
    data.extend_from_slice(&borsh_bytes(&withdraw_proof));
    data.extend_from_slice(&w_root);
    data.extend_from_slice(&nullifier_hash);
    data.extend_from_slice(&0u64.to_le_bytes()); // fee
    let withdraw_ix = || Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(pool, false),
            AccountMeta::new(nullifier, false),
            AccountMeta::new(recipient.pubkey(), false),
            AccountMeta::new(relayer.pubkey(), true),
            AccountMeta::new_readonly(system, false),
        ],
        data: data.clone(),
    };
    send(&mut svm, withdraw_ix(), &[&relayer]).expect("withdraw");

    let recipient_after = svm.get_balance(&recipient.pubkey()).unwrap_or(0);
    assert_eq!(
        recipient_after - recipient_before,
        DENOMINATION,
        "recipient should receive the full denomination",
    );

    // 4. double-spend: same nullifier must fail (nullifier PDA already exists).
    assert!(
        send(&mut svm, withdraw_ix(), &[&relayer]).is_err(),
        "second withdraw with the same nullifier must be rejected",
    );
}
