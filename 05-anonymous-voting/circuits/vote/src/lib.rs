//! Vote anonymously: prove that your identity commitment is on the voter list, a
//! Merkle tree rooted at `root`, without saying which one, and reveal the poll's
//! nullifier, so the program counts one vote per voter per poll. Public inputs, in
//! order: the [`Ballot`], `root, poll, nullifier, options, choice`. The program takes
//! `root`, `poll` and `options` from the poll's account and `nullifier` and `choice`
//! from the instruction, so a proof counts only for its poll, its voter list and its
//! choice.
//!
//! A voter is an [`Identity`], two random field elements. The list holds each
//! voter's commitment to it; the nullifier is a second function of it, scoped to the
//! poll: the same in every vote of one voter in one poll, and unrelated across polls
//! without the voter's `nullifier_key`.
//!
//! The list is the tree's leaves in order and the empty leaf after them, as 03's:
//! `xark-merkle`'s Poseidon2 tree, built in Rust with [`list`], a
//! `xark_merkle::tree::Tree`. [`Path`] is 03's too, and on the host (and in the
//! TypeScript client, through `#[input] impl`) builds the list's root and every
//! voter's path.
//!
//! Three hashes, three domains of Poseidon2: a commitment is `xark_poseidon2::commit`
//! and a nullifier `xark_poseidon2::nullifier`, the sponge with its capacity lane
//! started in a domain of its own, and a node is the compression
//! `xark_poseidon2::hash2`, with the capacity at 0. So no node is a voter's
//! commitment, and with the depth fixed, a node can't be passed off as a voter. A
//! nullifier and the commitment beside it are different functions of the
//! `nullifier_key`, so neither says anything about the other.
use xark::prelude::*;
use xark_merkle::poseidon2::merkle_root;
#[cfg(not(xark))]
use xark_merkle::tree::{self, Tree};
#[cfg(not(xark))]
use xark_poseidon2::hash2;
use xark_poseidon2::{commit, nullifier};

/// Tree depth: up to 2^20 voters. (`#[input]` arrays take a literal: `[Field; 20]`.)
pub const DEPTH: usize = 20;

/// The empty leaf, ASCII `xarkzero`: a constant no voter's commitment is, so the
/// padding can't vote.
pub const EMPTY_LEAF: u64 = 0x7861_726b_7a65_726f;

/// The empty voter list, as an authority builds it in Rust: push each voter's
/// commitment, then take its `root()` and each voter's `path(i)`.
#[cfg(not(xark))]
pub fn list() -> Tree<DEPTH> {
    Tree::poseidon2(Field::from(EMPTY_LEAF))
}

/// A voter's secret: whoever knows it votes as this voter. Two random field
/// elements, kept by the voter; the list holds only their commitment.
#[input]
pub struct Identity {
    pub nullifier_key: Field,
    pub trapdoor: Field,
}

#[input]
impl Identity {
    /// What the voter registers, and the list holds: `commit(nullifier_key,
    /// trapdoor)`.
    pub fn commitment(&self) -> Field {
        commit([self.nullifier_key, self.trapdoor])
    }

    /// What a vote in `poll` reveals: `nullifier(nullifier_key, poll)`. One per voter
    /// per poll; across polls, unlinkable without the `nullifier_key`.
    pub fn nullifier(&self, poll: Field) -> Field {
        nullifier([self.nullifier_key, poll])
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

/// What the program checks a vote against, the public inputs in this order. `root`,
/// `poll` and `options` come from the poll's account, `nullifier` and `choice` from
/// the instruction.
#[input]
pub struct Ballot {
    pub root: Field,
    pub poll: Field,
    pub nullifier: Field,
    pub options: u8,
    pub choice: u8,
}

#[input]
impl Ballot {
    /// `identity`, at `path`, voting `choice` of `options` in `poll`.
    pub fn of(identity: Identity, path: Path, poll: Field, options: u8, choice: u8) -> Ballot {
        Ballot {
            root: path.root(identity.commitment()),
            poll,
            nullifier: identity.nullifier(poll),
            options,
            choice,
        }
    }
}

/// The voter is on the list rooted at `ballot.root`, at the private `path`, this is
/// their nullifier in `ballot.poll`, and they chose one of the poll's options. Every
/// public input is in a constraint: `root` and `nullifier` are compared, `poll` is
/// hashed, and `options` and `choice` are `u8`s, range-checked on entry and then
/// ordered. No `bind` is needed.
#[circuit]
pub fn vote(identity: Private<Identity>, path: Private<Path>, ballot: Public<Ballot>) {
    require_eq(path.root(identity.commitment()), ballot.root);
    require_eq(identity.nullifier(ballot.poll), ballot.nullifier);
    require(ballot.choice < ballot.options);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(n: u64) -> Identity {
        Identity {
            nullifier_key: Field::from(n),
            trapdoor: Field::from(n + 1_000),
        }
    }

    /// The list's root and every voter's path, as a poll's authority builds them.
    fn of(voters: &[Identity]) -> (Field, Vec<Path>) {
        let mut tree = list();
        tree.extend(voters.iter().map(Identity::commitment));
        let paths = (0..voters.len()).map(|i| tree.path(i).into()).collect();
        (tree.root(), paths)
    }

    /// A voter's client keeps one path a voter at a time (`Path::first`, `next`,
    /// `update`), with no tree: it agrees with `Tree`'s, path for path.
    #[test]
    fn one_path_at_a_time_agrees_with_the_tree() {
        let leaves: Vec<Field> = (1..=11).map(|n| identity(n).commitment()).collect();
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
    fn every_voter_votes_for_any_option() {
        let voters = [identity(1), identity(2), identity(3)];
        let (root, paths) = of(&voters);
        for (path, v) in paths.iter().zip(voters) {
            for choice in 0..3 {
                let ballot = Ballot::of(v, *path, Field::from(7u64), 3, choice);
                assert_eq!(ballot.root, root);
                vote(v, *path, ballot).unwrap();
            }
        }
    }

    #[test]
    fn the_nullifier_is_one_per_voter_per_poll() {
        let (a, b) = (identity(1), identity(2));
        let (one, two) = (Field::from(1u64), Field::from(2u64));
        assert_ne!(a.nullifier(one), a.nullifier(two));
        assert_ne!(a.nullifier(one), b.nullifier(one));
        // Neither is the commitment the list holds.
        assert_ne!(a.nullifier(one), a.commitment());
    }

    #[test]
    fn nothing_else_votes() {
        let voters = [identity(1), identity(2), identity(3)];
        let (root, paths) = of(&voters);
        let poll = Field::from(7u64);
        let ok = Ballot::of(voters[0], paths[0], poll, 3, 1);
        vote(voters[0], paths[0], ok).unwrap();
        // Out of range: the poll's options are 0, 1 and 2.
        assert!(vote(voters[0], paths[0], Ballot { choice: 3, ..ok }).is_err());
        assert!(vote(voters[0], paths[0], Ballot { choice: 255, ..ok }).is_err());
        // Someone not on the list, at a voter's path or the next free one.
        let stranger = identity(9);
        let theirs = Ballot::of(stranger, paths[0], poll, 3, 1);
        assert!(vote(stranger, paths[0], Ballot { root, ..theirs }).is_err());
        // Another voter's nullifier, another poll's, another list.
        let nullifier = voters[1].nullifier(poll);
        assert!(vote(voters[0], paths[0], Ballot { nullifier, ..ok }).is_err());
        let poll = Field::from(8u64);
        assert!(vote(voters[0], paths[0], Ballot { poll, ..ok }).is_err());
        let (other, _) = of(&voters[..2]);
        assert!(vote(voters[0], paths[0], Ballot { root: other, ..ok }).is_err());
        // The padding after the list is empty leaves, which no voter's commitment is.
        let (_, longer) = of(&[voters[0], voters[1], voters[2], stranger]);
        let padded = Ballot::of(stranger, longer[3], Field::from(7u64), 3, 1);
        assert!(vote(stranger, longer[3], Ballot { root, ..padded }).is_err());
    }
}
