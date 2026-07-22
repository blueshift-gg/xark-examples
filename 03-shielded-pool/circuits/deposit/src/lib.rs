//! Shielded pool — DEPOSIT circuit. Proves `new_root` is the correct result of
//! appending `leaf` at `index` to the tree rooted at `old_root` — so the Solana
//! program never hashes (all Poseidon lives in the circuits, keeping circuit
//! and chain consistent by construction). The private `filled_subtrees`
//! frontier is pinned by the `old_root` check. Public inputs, in order:
//! old_root, new_root, leaf, index. (Design walkthrough in the README.)
use pool_merkle::{H, ZERO_LEAF, root_with_leaf, zeros};
use xark::prelude::*;

#[derive(Clone, Copy, Debug, CircuitInput)]
pub struct DepositWitness {
    pub filled_subtrees: [Field; H],
}

#[derive(Clone, Copy, Debug, CircuitInput)]
pub struct DepositStatement {
    pub old_root: Field,
    pub new_root: Field,
    pub leaf: Field,
    pub index: Field,
}

#[circuit]
pub fn shielded_pool_deposit(
    witness: Private<DepositWitness>,
    statement: Public<DepositStatement>,
) {
    // The low H bits of `index` drive the path; `to_bits` range-checks
    // index < 2^H, so a high index cannot silently alias a low slot.
    let index_bits = statement.index.to_bits::<H>();
    let z = zeros();

    // 1. Bind the private frontier to the chain's current root. Only the true
    //    frontier reproduces `old_root` (collision resistance of Poseidon2).
    require_eq(
        root_with_leaf(
            Field::from(ZERO_LEAF),
            index_bits,
            witness.filled_subtrees,
            z,
        ),
        statement.old_root,
    );

    // 2. Prove `new_root` = the same tree with `leaf` inserted at `index`.
    require_eq(
        root_with_leaf(statement.leaf, index_bits, witness.filled_subtrees, z),
        statement.new_root,
    );
}
