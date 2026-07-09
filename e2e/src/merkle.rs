//! Native mirror of the circuits' incremental Merkle tree (height 20,
//! `poseidon2::hash2` nodes) — what a wallet runs to build witnesses: frontiers,
//! authentication paths, roots. Kept in lock-step with the `pool-merkle`
//! circuit gadget; the e2e flows fail loudly if the two ever disagree, because
//! the proofs stop verifying.

use ark_bn254::Fr;
use ark_ff::Zero;

use crate::poseidon2::hash2;

pub const H: usize = 20;
pub const EMPTY_LEAF: u64 = 0x7861_726b_7a65_726f;

/// `zeros()[i]` = root of an all-empty subtree of height `i` (`zeros[0]` is the
/// nonzero empty-leaf sentinel).
pub fn zeros() -> [Fr; H + 1] {
    let mut z = [Fr::zero(); H + 1];
    z[0] = Fr::from(EMPTY_LEAF);
    for i in 1..=H {
        z[i] = hash2(z[i - 1], z[i - 1]);
    }
    z
}

/// An append-only incremental Merkle tree (Tornado's MerkleTreeWithHistory):
/// stores only the left-sibling frontier, appends in O(H).
pub struct IncrementalTree {
    pub zeros: [Fr; H + 1],
    /// Left siblings along the path to the next free slot.
    pub filled: [Fr; H],
    pub next_index: u32,
    /// All leaves ever appended (a wallet keeps them to build auth paths).
    pub leaves: Vec<Fr>,
}

impl Default for IncrementalTree {
    fn default() -> Self {
        Self::new()
    }
}

impl IncrementalTree {
    pub fn new() -> Self {
        let zeros = zeros();
        Self {
            zeros,
            filled: [Fr::zero(); H],
            next_index: 0,
            leaves: Vec::new(),
        }
    }

    /// The current root (empty slots below the frontier).
    pub fn root(&self) -> Fr {
        self.root_with(Fr::from(EMPTY_LEAF), self.next_index)
    }

    /// Root when `leaf` sits at `index` and everything after is empty, using
    /// the current frontier — the circuits' `root_with_leaf`.
    pub fn root_with(&self, leaf: Fr, index: u32) -> Fr {
        let mut current = leaf;
        let mut idx = index;
        for i in 0..H {
            current = if idx & 1 == 0 {
                hash2(current, self.zeros[i])
            } else {
                hash2(self.filled[i], current)
            };
            idx >>= 1;
        }
        current
    }

    /// The frontier snapshot a deposit proof needs, then append the leaf.
    pub fn append(&mut self, leaf: Fr) -> [Fr; H] {
        let frontier = self.filled;
        let mut current = leaf;
        let mut idx = self.next_index;
        for i in 0..H {
            if idx & 1 == 0 {
                self.filled[i] = current;
                current = hash2(current, self.zeros[i]);
            } else {
                current = hash2(self.filled[i], current);
            }
            idx >>= 1;
        }
        self.next_index += 1;
        self.leaves.push(leaf);
        frontier
    }

    /// Authentication path (sibling per level + LSB-first direction bits) for
    /// the leaf at `index`, against the *current* tree.
    pub fn auth_path(&self, index: u32) -> ([Fr; H], [u8; H]) {
        let mut path = [Fr::zero(); H];
        let mut bits = [0u8; H];
        // Recompute each level's siblings from the stored leaves (fine for the
        // handful of leaves the examples use).
        let mut level: Vec<Fr> = self.leaves.clone();
        let mut idx = index as usize;
        for i in 0..H {
            bits[i] = (idx & 1) as u8;
            let sib = idx ^ 1;
            path[i] = level.get(sib).copied().unwrap_or(self.zeros[i]);
            let mut next = Vec::with_capacity(level.len().div_ceil(2));
            for pair in level.chunks(2) {
                let l = pair[0];
                let r = pair.get(1).copied().unwrap_or(self.zeros[i]);
                next.push(hash2(l, r));
            }
            level = next;
            idx >>= 1;
        }
        (path, bits)
    }
}
