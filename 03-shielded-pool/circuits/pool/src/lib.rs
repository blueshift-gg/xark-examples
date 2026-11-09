//! What the pool's two circuits share, and 04's: paths into the Merkle tree of
//! notes, and Solana addresses as circuit inputs.
//!
//! The tree is `xark-merkle`'s, whose membership fold is formally verified: every
//! root a circuit computes is `xark_merkle::merkle_root`. A wallet tracks the tree
//! as the path of its next free leaf: that path's root with the empty leaf is the
//! tree's root, and with a note it's the root once the note is appended. So one
//! type, [`Path`], is what a deposit proves, what a withdrawal proves, and what a
//! wallet keeps.
//!
//! The same code runs in the circuits and in the wallet. In a circuit, `Path::root`
//! is constraints; on the host (and in the TypeScript client, through `#[input]
//! impl`) it computes the roots a wallet proves, so the two can't disagree.
//! `Path::first`, `next` and `update` are host-only: a wallet keeps its paths
//! current as notes land on-chain.
//!
//! Tree nodes are Poseidon (`xark_poseidon::hash2`, as `xark-merkle` hashes them);
//! note commitments and nullifiers are Poseidon2 (`xark_poseidon2::hash`), another
//! permutation, so a node can't be read as either.
use xark::prelude::*;

/// Tree height: 2^20 notes. (`#[input]` arrays take a literal: `[Field; 20]`.)
pub const H: usize = 20;

/// The empty leaf, ASCII `xarkzero`: a constant no note commitment is, so an empty
/// slot can't be spent.
pub const ZERO_LEAF: u64 = 0x7861_726b_7a65_726f;

/// A leaf's authentication path: its index, and its sibling at each height.
#[input]
pub struct Path {
    pub index: u32,
    pub siblings: [Field; 20],
}

#[input]
impl Path {
    /// The first leaf's path, in the empty tree: every sibling is empty.
    #[cfg(not(xark))]
    pub fn first() -> Path {
        Path {
            index: 0,
            siblings: zeros(),
        }
    }

    /// The root of the tree with `leaf` at this path: for the next free leaf's path,
    /// the tree's root once `leaf` is appended. In a circuit this binds `index`
    /// below 2^20.
    pub fn root(&self, leaf: Field) -> Field {
        let bits = Field::from(self.index).to_bits::<H>();
        xark_merkle::merkle_root(leaf, self.siblings, bits)
    }

    /// For the next free leaf's path, the tree's root: the slot is still empty.
    pub fn tree_root(&self) -> Field {
        self.root(Field::from(ZERO_LEAF))
    }

    /// The path of the leaf after this one, once `leaf` is at this path: for the
    /// next free leaf's path, the next free leaf's path after appending `leaf`.
    #[cfg(not(xark))]
    pub fn next(self, leaf: Field) -> Path {
        assert!((self.index as usize) < (1 << H) - 1, "the tree is full");
        let index = self.index + 1;
        let z = zeros();
        let mut siblings = self.siblings;
        let mut node = leaf;
        for (i, sibling) in siblings.iter_mut().enumerate() {
            let (mine, next) = (self.index >> i, index >> i);
            let old = *sibling;
            if next != mine {
                // A new subtree: its left sibling is this leaf's, its right is empty.
                *sibling = if next & 1 == 1 { node } else { z[i] };
            }
            node = node_of(mine, node, old);
        }
        Path { index, siblings }
    }

    /// This path once `leaf` is at `later`, a later leaf's path: the subtree beside
    /// this path that `leaf` lands in is the one sibling it changes.
    #[cfg(not(xark))]
    pub fn update(self, later: Path, leaf: Field) -> Path {
        assert!(
            later.index > self.index,
            "`later` must be a later leaf's path"
        );
        let mut siblings = self.siblings;
        let mut node = leaf;
        for (i, (sibling, beside)) in siblings.iter_mut().zip(later.siblings).enumerate() {
            let at = later.index >> i;
            if at == self.index >> i ^ 1 {
                *sibling = node;
            }
            node = node_of(at, node, beside);
        }
        Path {
            index: self.index,
            siblings,
        }
    }
}

/// The parent of `node`, at position `at` in its level, and its `sibling`: the
/// node `xark_merkle::merkle_root` computes at each height.
#[cfg(not(xark))]
fn node_of(at: u32, node: Field, sibling: Field) -> Field {
    if at & 1 == 0 {
        xark_poseidon::hash2(node, sibling)
    } else {
        xark_poseidon::hash2(sibling, node)
    }
}

/// `zeros()[i]` is the root of an empty subtree of height `i`.
#[cfg(not(xark))]
fn zeros() -> [Field; H] {
    let mut z = [Field::from(ZERO_LEAF); H];
    for i in 1..H {
        z[i] = xark_poseidon::hash2(z[i - 1], z[i - 1]);
    }
    z
}

/// A Solana address (or any 32 bytes) is xark's `Hash`: two field elements, each
/// 16-byte half read as a big-endian integer. Clients give its 32 bytes, and programs
/// split an address and a hash the same way. (Written as `Hash`, not an alias: a
/// client resolves an exported function's types by the name written.)
pub use xark_hash::Hash;

/// Its two field elements, `[hi, lo]`: to hash it, or to bind it.
pub fn halves(x: Hash) -> [Field; 2] {
    let mut halves = [Field::from(0u64); 2];
    xark::input::write_fields(&x, &mut halves);
    halves
}

/// Binds a 32-byte public input (an address, a hash) that the circuit doesn't
/// otherwise use to the proof, as Tornado's `recipientSquare`: Groth16 checks a public
/// input only through the constraints that use it, and one in none would verify with
/// any value (`xark build` refuses it). One constraint per half.
pub fn bind(x: Hash) {
    for half in halves(x) {
        let _ = half * half;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xark_poseidon::hash2;

    /// The root of `leaves` from scratch, level by level.
    fn root_of(leaves: &[Field]) -> Field {
        let mut level = leaves.to_vec();
        let mut empty = Field::from(ZERO_LEAF);
        for _ in 0..H {
            let pairs = level
                .chunks(2)
                .map(|p| hash2(p[0], *p.get(1).unwrap_or(&empty)));
            level = pairs.collect();
            empty = hash2(empty, empty);
        }
        level.first().copied().unwrap_or(empty)
    }

    #[test]
    fn paths_agree_with_the_whole_tree() {
        let leaves: Vec<Field> = (1..=11u64).map(Field::from).collect();
        let mut tip = Path::first(); // the next free leaf's path
        let mut paths: Vec<Path> = Vec::new();
        assert_eq!(tip.tree_root(), root_of(&[]));
        for (i, &leaf) in leaves.iter().enumerate() {
            assert_eq!(tip.root(leaf), root_of(&leaves[..=i]));
            paths = paths.into_iter().map(|p| p.update(tip, leaf)).collect();
            paths.push(tip);
            tip = tip.next(leaf);
            assert_eq!(tip.index as usize, i + 1);
            assert_eq!(tip.tree_root(), root_of(&leaves[..=i]));
            // Every leaf so far proves membership in the current root.
            for (j, path) in paths.iter().enumerate() {
                assert_eq!(
                    path.root(leaves[j]),
                    root_of(&leaves[..=i]),
                    "leaf {j} after {i}"
                );
            }
        }
    }
}
