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
#![no_std]

use pool_merkle::{merkle_root, H};
use xark::prelude::*;
use xark_poseidon2::hash;

#[allow(clippy::too_many_arguments)]
pub fn circuit(
    secret: Private<Field>,
    nullifier: Private<Field>,
    path_elements: Private<[Field; H]>,
    path_index_bits: Private<[Field; H]>, // 0 = current node is a left child, 1 = right
    root: Public<Field>,
    nullifier_hash: Public<Field>,
    recipient_hi: Public<Field>,
    recipient_lo: Public<Field>,
    relayer_hi: Public<Field>,
    relayer_lo: Public<Field>,
    fee: Public<Field>,
) {
    // 1. Recompute the commitment from its secret parts.
    let commitment = hash::<2>([nullifier, secret]);

    // 2. Prove the commitment is a leaf in the tree rooted at `root`
    //    (booleanity of the index bits is asserted inside merkle_root).
    assert_eq(
        merkle_root(commitment, path_elements, path_index_bits),
        root,
    );

    // 3. Reveal the nullifier hash (the pool marks it spent to prevent re-use).
    assert_eq(hash::<1>([nullifier]), nullifier_hash);

    // 4. Binding to a specific recipient/relayer comes from Groth16 over the
    //    hi/lo halves the program derives from real pubkeys. Range-check each
    //    half to 128 bits and the fee to 64 (`to_bits` enforces the bound),
    //    matching the program's split exactly.
    let _ = recipient_hi.to_bits::<128>();
    let _ = recipient_lo.to_bits::<128>();
    let _ = relayer_hi.to_bits::<128>();
    let _ = relayer_lo.to_bits::<128>();
    let _ = fee.to_bits::<64>();
}
