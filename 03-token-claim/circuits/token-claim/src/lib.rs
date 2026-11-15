//! Claim an airdrop: prove that `(claimant, amount)` is in the distributor's list,
//! a Merkle tree rooted at `root`, without putting the path onchain. Public inputs,
//! in order: `root`, `claim` (`claimant` as two halves, then `amount`). The program
//! takes `root` from its state, `claimant` from the transaction's signer and
//! `amount` from the instruction, so a proof pays only the wallet it was made for,
//! only its amount, and only from its list.
//!
//! The list is the tree's leaves in order, one per claim, and the empty leaf after
//! them: a list of any length up to 2^20 has the one root. The tree is
//! `xark-merkle`'s Poseidon2 tree: the circuit folds a path with
//! `xark_merkle::poseidon2::merkle_root`, and a distributor in Rust builds the list
//! with `xark_merkle::tree::Tree`. [`Path`] is a leaf's authentication path, private
//! to the circuit; on the host (and in the TypeScript client, through `#[input]
//! impl`) it also builds the list's root and every claimant's path one claim at a
//! time, so the distributor and the claimant hash as the circuit does.
//!
//! Leaves and nodes are both Poseidon2, as two different functions: a leaf is the
//! sponge `xark_poseidon2::hash` over its three fields, whose capacity lane starts
//! at the length, 3, and which runs the permutation twice; a node is the 2-to-1
//! compression `xark_poseidon2::hash2`, one permutation with the capacity at 0. With
//! the depth fixed, a node can't be passed off as a claim.
use xark::prelude::*;
use xark_merkle::poseidon2::merkle_root;
#[cfg(not(xark))]
use xark_merkle::tree::{self, Tree};
#[cfg(not(xark))]
use xark_poseidon2::hash2;

/// Tree depth: up to 2^20 claims. (`#[input]` arrays take a literal: `[Field; 20]`.)
pub const DEPTH: usize = 20;

/// The empty leaf, ASCII `xarkzero`: a constant no claim's leaf is, so the padding
/// can't be claimed.
pub const EMPTY_LEAF: u64 = 0x7861_726b_7a65_726f;

/// The empty list, as a distributor builds it in Rust: push each claim's leaf, then
/// take its `root()` and each claim's `path(i)`.
#[cfg(not(xark))]
pub fn list() -> Tree<DEPTH> {
    Tree::poseidon2(Field::from(EMPTY_LEAF))
}

/// One entry of the list: `amount` of the token for the wallet `claimant`.
#[input]
pub struct Claim {
    pub claimant: Address,
    pub amount: u64,
}

#[input]
impl Claim {
    /// The leaf the claim is in the list as: `Poseidon2(claimant.hi, claimant.lo,
    /// amount)`.
    pub fn leaf(&self) -> Field {
        let [hi, lo] = self.claimant.to_fields();
        xark_poseidon2::hash([hi, lo, Field::from(self.amount)])
    }
}

/// A leaf's authentication path: its index in the list, and its sibling at each
/// height, leaf level first.
#[input]
pub struct Path {
    pub index: u32,
    pub siblings: [Field; 20],
}

#[input]
impl Path {
    /// The first leaf's path, in the empty list: every sibling is empty.
    #[cfg(not(xark))]
    pub fn first() -> Path {
        list().path(0).into()
    }

    /// The root of the tree with `leaf` at this path. In a circuit this binds `index`
    /// below 2^20, and `merkle_root` each of its bits to 0 or 1.
    pub fn root(&self, leaf: Field) -> Field {
        merkle_root(
            leaf,
            self.siblings,
            Field::from(self.index).to_bits::<DEPTH>(),
        )
    }

    /// For the next free leaf's path, the root of the list so far: every leaf from
    /// this one on is empty.
    #[cfg(not(xark))]
    pub fn list_root(&self) -> Field {
        self.root(Field::from(EMPTY_LEAF))
    }

    /// The path of the leaf after this one, once `leaf` is at this path: for the next
    /// free leaf's path, the next free leaf's path after adding `leaf` to the list.
    /// (A Rust client that holds the whole list uses [`list`], a `Tree`, instead.)
    #[cfg(not(xark))]
    pub fn next(self, leaf: Field) -> Path {
        assert!((self.index as usize) < (1 << DEPTH) - 1, "the list is full");
        let index = self.index + 1;
        let empty = list();
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

/// A path out of a `Tree`, as the circuit takes it.
#[cfg(not(xark))]
impl From<tree::Path<DEPTH>> for Path {
    fn from(path: tree::Path<DEPTH>) -> Path {
        Path {
            index: path.index as u32,
            siblings: path.siblings,
        }
    }
}

/// The parent of `node`, at position `at` in its level, and its `sibling`: the node
/// `Path::root` computes at each height.
#[cfg(not(xark))]
fn node_of(at: u32, node: Field, sibling: Field) -> Field {
    if at & 1 == 0 {
        hash2(node, sibling)
    } else {
        hash2(sibling, node)
    }
}

/// The claim is in the list rooted at `root`, at the private `path`. `claimant` and
/// `amount` are public inputs the leaf hashes, so the proof holds only for them:
/// no `bind` is needed.
#[circuit]
pub fn token_claim(path: Private<Path>, root: Public<Field>, claim: Public<Claim>) {
    require_eq(path.root(claim.leaf()), root);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claim(n: u8, amount: u64) -> Claim {
        Claim {
            claimant: [n; 32].into(),
            amount,
        }
    }

    /// The list's root and every claim's path, as a distributor builds them.
    fn of(claims: &[Claim]) -> (Field, Vec<Path>) {
        let mut tree = list();
        tree.extend(claims.iter().map(Claim::leaf));
        let paths = (0..claims.len()).map(|i| tree.path(i).into()).collect();
        (tree.root(), paths)
    }

    /// A claimant's client keeps one path a claim at a time (`Path::first`, `next`,
    /// `update`), with no tree: it agrees with `Tree`'s, path for path.
    #[test]
    fn one_path_at_a_time_agrees_with_the_tree() {
        let leaves: Vec<Field> = (1..=11)
            .map(|n| claim(n, u64::from(n) * 100).leaf())
            .collect();
        let mut tree = list();
        let mut tip = Path::first(); // the next free leaf's path
        let mut paths: Vec<Path> = Vec::new();
        assert_eq!(tip.list_root(), tree.root());
        for &leaf in &leaves {
            paths = paths.into_iter().map(|p| p.update(tip, leaf)).collect();
            paths.push(tip);
            tip = tip.next(leaf);
            tree.push(leaf);
            assert_eq!(tip.list_root(), tree.root());
            for (i, path) in paths.iter().chain([&tip]).enumerate() {
                let want = tree.path(i);
                assert_eq!(
                    (path.index as usize, path.siblings),
                    (want.index, want.siblings)
                );
            }
        }
    }

    #[test]
    fn everyone_on_the_list_claims_their_amount() {
        let claims = [claim(1, 500), claim(2, 1_000), claim(3, 250)];
        let (root, paths) = of(&claims);
        for (path, c) in paths.iter().zip(claims) {
            token_claim(*path, root, c).unwrap();
        }
    }

    #[test]
    fn nothing_else_is_on_the_list() {
        let claims = [claim(1, 500), claim(2, 1_000), claim(3, 250)];
        let (root, paths) = of(&claims);
        // Another amount, someone else's path, an address not on the list, another list.
        assert!(token_claim(paths[0], root, claim(1, 501)).is_err());
        assert!(token_claim(paths[1], root, claim(1, 500)).is_err());
        assert!(token_claim(paths[0], root, claim(9, 500)).is_err());
        let (other, _) = of(&[claim(1, 500)]);
        assert!(token_claim(paths[0], other, claims[0]).is_err());
        // The padding after the list is empty leaves, which no claim hashes to.
        let (_, longer) = of(&[claim(1, 500), claim(2, 1_000), claim(3, 250), claim(4, 1)]);
        assert!(token_claim(longer[3], root, claim(4, 1)).is_err());
    }
}
