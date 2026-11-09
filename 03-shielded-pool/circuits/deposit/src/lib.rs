//! Shielded pool: the DEPOSIT circuit. Proves the program's tree with `leaf`
//! appended at its next index has root `new_root`, so the program can advance its
//! tree without hashing: all hashing is in the circuits.
use pool::Path;
use xark::prelude::*;

/// What the program checks a deposit against: its tree's root and next index, the
/// leaf, and the root once the leaf is appended. The public inputs, in this order.
#[input]
pub struct Append {
    pub old_root: Field,
    pub new_root: Field,
    pub leaf: Field,
    pub index: u32,
}

#[input]
impl Append {
    /// `leaf` appended at `path`, the next free leaf's path.
    pub fn of(path: Path, leaf: Field) -> Append {
        Append {
            old_root: path.tree_root(),
            new_root: path.root(leaf),
            leaf,
            index: path.index,
        }
    }
}

/// `path` is the next free leaf's. Only the tree's true nodes reproduce the
/// program's root (Poseidon's collision resistance), so the path is the true path,
/// and `new_root` is the program's tree with `leaf` in the next slot.
#[circuit]
pub fn shielded_pool_deposit(path: Private<Path>, append: Public<Append>) {
    require_eq(Append::of(path, append.leaf), append);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_to_the_tree() {
        let tip = Path::first().next(Field::from(7u64));
        let leaf = Field::from(8u64);
        let append = Append::of(tip, leaf);
        shielded_pool_deposit(tip, append).unwrap();
        // Another index, another tree, another leaf: each is refused.
        assert!(shielded_pool_deposit(tip, Append { index: 2, ..append }).is_err());
        let empty = Path::first().tree_root();
        assert!(
            shielded_pool_deposit(
                tip,
                Append {
                    old_root: empty,
                    ..append
                }
            )
            .is_err()
        );
        let other = Field::from(9u64);
        assert!(
            shielded_pool_deposit(
                tip,
                Append {
                    leaf: other,
                    ..append
                }
            )
            .is_err()
        );
        // The first slot again, over the leaf already there.
        assert!(shielded_pool_deposit(Path::first(), Append { index: 0, ..append }).is_err());
    }
}
