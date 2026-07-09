//! Shielded pool — DEPOSIT circuit. Proves `new_root` is the correct result of
//! appending `leaf` at `index` to the tree rooted at `old_root` — so the Solana
//! program never hashes (all Poseidon lives in the circuits, keeping circuit
//! and chain consistent by construction). The private `filled_subtrees`
//! frontier is pinned by the `old_root` check. Public inputs, in order:
//! old_root, new_root, leaf, index. (Design walkthrough in the README.)
#![no_std]

use pool_merkle::{root_with_leaf, zeros, H, ZERO_LEAF};
use xark::prelude::*;

pub fn circuit(
    filled_subtrees: Private<[Field; H]>,
    old_root: Public<Field>,
    new_root: Public<Field>,
    leaf: Public<Field>,
    index: Public<Field>,
) {
    // The low H bits of `index` drive the path; `to_bits` range-checks
    // index < 2^H, so a high index cannot silently alias a low slot.
    let index_bits = index.to_bits::<H>();
    let z = zeros();

    // 1. Bind the private frontier to the chain's current root. Only the true
    //    frontier reproduces `old_root` (collision resistance of Poseidon2).
    assert_eq(
        root_with_leaf(Field::from(ZERO_LEAF), index_bits, filled_subtrees, z),
        old_root,
    );

    // 2. Prove `new_root` = the same tree with `leaf` inserted at `index`.
    assert_eq(
        root_with_leaf(leaf, index_bits, filled_subtrees, z),
        new_root,
    );
}
