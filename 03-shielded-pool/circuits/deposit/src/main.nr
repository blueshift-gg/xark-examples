// Shielded pool — DEPOSIT circuit.
//
// The insight that makes this design clean: the Solana program never hashes.
// It stores only { current_root, next_index, recent-roots ring }. To add a
// commitment to the tree, the depositor proves — in Noir — that a `new_root`
// is the correct result of appending `leaf` at `index` to the tree the chain
// currently roots at `old_root`. Because ALL Poseidon happens here (and in the
// withdraw circuit), the circuit and chain can never disagree about hashes.
//
// How the depositor knows `filled_subtrees` (the frontier / left-siblings along
// the insertion path): they reconstruct it from the public `Deposit` events —
// every commitment ever inserted is on-chain. A wrong frontier can't pass the
// `old_root` check below (that would require a Poseidon collision).
//
// Public inputs (IN THIS ORDER — the program builds them identically):
//   old_root, new_root, leaf, index
// Private inputs:
//   filled_subtrees
//
// Verify the ordering against `xark inspect` before wiring up the program.

use std::hash::poseidon2::Poseidon2;

global H: u32 = 20; // tree height → 2^20 = 1,048,576 leaves
global ZERO: Field = 0; // empty-leaf sentinel (see README: production uses a nonzero NUMS value)

fn main(
    filled_subtrees: [Field; H],
    old_root: pub Field,
    new_root: pub Field,
    leaf: pub Field,
    index: pub u32,
) {
    let zeros = zeros_array();

    // 1. Bind the private frontier to the chain's current root. Only the true
    //    frontier reproduces `old_root` (collision resistance of Poseidon2).
    let recomputed_old = root_with_leaf(ZERO, index, filled_subtrees, zeros);
    assert(recomputed_old == old_root);

    // 2. Prove `new_root` = the same tree with `leaf` inserted at `index`.
    let recomputed_new = root_with_leaf(leaf, index, filled_subtrees, zeros);
    assert(recomputed_new == new_root);
}

fn hash2(l: Field, r: Field) -> Field {
    Poseidon2::hash([l, r], 2)
}

// Root of the tree when position `index` holds `leaf`, everything after it is
// empty, and `filled_subtrees` are the left-siblings along the path. This is
// the standard incremental-Merkle recomputation (à la Tornado's
// MerkleTreeWithHistory), lifted into a circuit.
fn root_with_leaf(leaf: Field, index: u32, filled: [Field; H], zeros: [Field; H]) -> Field {
    let mut current = leaf;
    let mut idx = index;
    for i in 0..H {
        let bit = idx & 1;
        if bit == 0 {
            // current node is a LEFT child; its right sibling is an empty subtree
            current = hash2(current, zeros[i]);
        } else {
            // current node is a RIGHT child; its left sibling is already filled
            current = hash2(filled[i], current);
        }
        idx = idx >> 1;
    }
    current
}

// zeros[i] = root of an all-empty subtree of height i.
fn zeros_array() -> [Field; H] {
    let mut z: [Field; H] = [0; H];
    z[0] = ZERO;
    for i in 1..H {
        z[i] = hash2(z[i - 1], z[i - 1]);
    }
    z
}
