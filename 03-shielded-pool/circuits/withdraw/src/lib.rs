//! Shielded pool — WITHDRAW circuit. Proves membership of your commitment in a
//! remembered `root` without revealing which leaf, and reveals `nullifier_hash`
//! so the pool can mark the note spent. recipient/relayer/fee are public
//! inputs, so Groth16 binds them to the proof — nobody can re-target the
//! withdrawal. Pubkeys exceed the BN254 field, so each is split into two
//! 128-bit halves (hi/lo); the program splits them identically.
//!
//! Poseidon2 domains: commitment = `hash::<2>([nullifier, secret])` (capacity
//! tag 2), nullifier hash = `hash::<1>([nullifier])` (tag 1), tree nodes =
//! `hash2` (tag 0) — three separate domains, no cross-collisions.
//!
//! Public inputs, in order: root, nullifier_hash, recipient_hi, recipient_lo,
//! relayer_hi, relayer_lo, fee. Private: secret, nullifier, path_elements,
//! path_index_bits. (See the README.)
use pool_merkle::{H, merkle_root};
use xark::prelude::*;
use xark_poseidon2::hash;

#[derive(Clone, Copy, Debug, CircuitInput)]
pub struct WithdrawWitness {
    pub secret: Field,
    pub nullifier: Field,
    pub path_elements: [Field; H],
    /// 0 = current node is a left child, 1 = right.
    pub path_index_bits: [Field; H],
}

#[derive(Clone, Copy, Debug, CircuitInput)]
pub struct WithdrawStatement {
    pub root: Field,
    pub nullifier_hash: Field,
    pub recipient_hi: Field,
    pub recipient_lo: Field,
    pub relayer_hi: Field,
    pub relayer_lo: Field,
    pub fee: Field,
}

#[circuit]
pub fn shielded_pool_withdraw(
    witness: Private<WithdrawWitness>,
    statement: Public<WithdrawStatement>,
) {
    // 1. Recompute the commitment from its secret parts.
    let commitment = hash::<2>([witness.nullifier, witness.secret]);

    // 2. Prove the commitment is a leaf in the tree rooted at `root`
    //    (booleanity of the index bits is asserted inside merkle_root).
    require_eq(
        merkle_root(commitment, witness.path_elements, witness.path_index_bits),
        statement.root,
    );

    // 3. Reveal the nullifier hash (the pool marks it spent to prevent re-use).
    require_eq(hash::<1>([witness.nullifier]), statement.nullifier_hash);

    // 4. Binding to a specific recipient/relayer comes from Groth16 over the
    //    hi/lo halves the program derives from real pubkeys. Range-check each
    //    half to 128 bits and the fee to 64 (`to_bits` enforces the bound),
    //    matching the program's split exactly.
    let _ = statement.recipient_hi.to_bits::<128>();
    let _ = statement.recipient_lo.to_bits::<128>();
    let _ = statement.relayer_hi.to_bits::<128>();
    let _ = statement.relayer_lo.to_bits::<128>();
    let _ = statement.fee.to_bits::<64>();
}
