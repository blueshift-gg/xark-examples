//! What the pool's two circuits share, and 06's: paths into the Merkle tree of
//! notes.
//!
//! The tree is `xark-merkle`'s Poseidon2 tree (`xark_merkle::poseidon2`), each
//! level a formally verified `cond_swap` and Poseidon2's `hash2`: every
//! root a circuit computes is its `merkle_root`, and every append its
//! `merkle_append`. A wallet tracks the tree as the path of its next free leaf: that
//! path's root with the empty leaf is the tree's root, and with a note it's the root
//! once the note is appended. So one type, [`Path`], is what a deposit proves, what
//! a withdrawal proves, and what a wallet keeps.
//!
//! The same code runs in the circuits and in the wallet. In a circuit, `Path::root`
//! is constraints; on the host (and in the TypeScript client, through `#[input]
//! impl`) it computes the roots a wallet proves, so the two can't disagree.
//! `Path::first`, `next` and `update` are host-only: a wallet keeps its paths
//! current as notes land onchain. A host that holds every note builds the tree with
//! [`tree`], `xark_merkle::tree`'s builder, instead.
//!
//! Tree nodes are Poseidon2's 2-to-1 compression `hash2`, with its capacity at 0;
//! note commitments and nullifiers are Poseidon2's `commit` and `nullifier`, each
//! in a domain of its own, so a node can't be read as either.
use xark::prelude::*;
use xark_merkle::poseidon2::{merkle_append, merkle_root};
#[cfg(not(xark))]
use xark_merkle::tree::{self, Tree};
#[cfg(not(xark))]
use xark_poseidon2::hash2;

/// Tree height: 2^20 notes. (`#[input]` arrays take a literal: `[Field; 20]`.)
pub const H: usize = 20;

/// The empty leaf, ASCII `xarkzero`: a constant no note commitment is, so an empty
/// slot can't be spent.
pub const ZERO_LEAF: u64 = 0x7861_726b_7a65_726f;

/// The empty tree, as `xark_merkle::tree` builds it on the host: push each note's
/// commitment, then take its `root()` and a note's `path(i)`.
#[cfg(not(xark))]
pub fn tree() -> Tree<H> {
    Tree::poseidon2(Field::from(ZERO_LEAF))
}

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
        tree().path(0).into()
    }

    /// The root of the tree with `leaf` at this path: for the next free leaf's path,
    /// the tree's root once `leaf` is appended. In a circuit this binds `index`
    /// below 2^20.
    pub fn root(&self, leaf: Field) -> Field {
        merkle_root(leaf, self.siblings, bits(self.index))
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
        let empty = tree();
        let mut siblings = self.siblings;
        let mut node = leaf;
        for (i, sibling) in siblings.iter_mut().enumerate() {
            let (mine, next) = (self.index >> i, index >> i);
            let old = *sibling;
            if next != mine {
                // A new subtree: its left sibling is this leaf's, its right is empty.
                *sibling = if next & 1 == 1 {
                    node
                } else {
                    empty.empty_subtrees()[i]
                };
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

impl Path {
    /// In a circuit: this path's slot holds the empty leaf in the tree rooted at
    /// `old_root`, and the result is that root with `leaf` written there. One path
    /// proves both (`merkle_append`), so the new tree is the old one with one slot
    /// filled.
    pub fn append(&self, old_root: Field, leaf: Field) -> Field {
        merkle_append(
            old_root,
            Field::from(ZERO_LEAF),
            leaf,
            self.siblings,
            bits(self.index),
        )
    }

    /// In a circuit: this path's slot is even and it and the slot after it are empty
    /// in the tree rooted at `old_root`, and the result is that root with `pair`
    /// written in them. The pair is one leaf of the tree of pairs, the subtrees of
    /// height 1, whose empty leaf is `hash2(ZERO_LEAF, ZERO_LEAF)`: `merkle_append`
    /// on the path's upper 19 levels.
    pub fn append_pair(&self, old_root: Field, pair: [Field; 2]) -> Field {
        let bits = bits(self.index);
        require_eq(bits[0], Field::from(0u64));
        let mut siblings = [Field::from(0u64); H - 1];
        let mut up = [Field::from(0u64); H - 1];
        let mut i = 0;
        while i < H - 1 {
            siblings[i] = self.siblings[i + 1];
            up[i] = bits[i + 1];
            i += 1;
        }
        let empty = Field::from(ZERO_LEAF);
        merkle_append(
            old_root,
            xark_poseidon2::hash2(empty, empty),
            xark_poseidon2::hash2(pair[0], pair[1]),
            siblings,
            up,
        )
    }
}

/// A path out of `tree()`, as the circuits take it.
#[cfg(not(xark))]
impl From<tree::Path<H>> for Path {
    fn from(path: tree::Path<H>) -> Path {
        Path {
            index: path.index as u32,
            siblings: path.siblings,
        }
    }
}

/// `index` as the tree's `H` bits, least significant first. In a circuit this binds
/// `index` below 2^20.
fn bits(index: u32) -> [Field; H] {
    Field::from(index).to_bits::<H>()
}

/// The parent of `node`, at position `at` in its level, and its `sibling`: the
/// node `merkle_root` computes at each height.
#[cfg(not(xark))]
fn node_of(at: u32, node: Field, sibling: Field) -> Field {
    if at & 1 == 0 {
        hash2(node, sibling)
    } else {
        hash2(sibling, node)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A wallet keeps one path a note at a time (`Path::first`, `next`, `update`),
    /// with no tree: it agrees with `xark_merkle::tree`'s, path for path.
    #[test]
    fn paths_agree_with_the_tree() {
        let leaves: Vec<Field> = (1..=11u64).map(Field::from).collect();
        let mut tree = tree();
        let mut tip = Path::first(); // the next free leaf's path
        let mut paths: Vec<Path> = Vec::new();
        assert_eq!(tip.tree_root(), tree.root());
        for &leaf in &leaves {
            assert_eq!(tip.root(leaf), {
                let mut after = tree.clone();
                after.push(leaf);
                after.root()
            });
            paths = paths.into_iter().map(|p| p.update(tip, leaf)).collect();
            paths.push(tip);
            tip = tip.next(leaf);
            tree.push(leaf);
            assert_eq!(tip.tree_root(), tree.root());
            for (i, path) in paths.iter().chain([&tip]).enumerate() {
                let want = tree.path(i);
                assert_eq!(
                    (path.index as usize, path.siblings),
                    (want.index, want.siblings)
                );
            }
        }
    }
}
