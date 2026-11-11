//! Shielded pool: the WITHDRAW circuit. Proves the note behind some leaf of the
//! tree rooted at `root` is yours, without saying which leaf, and reveals its
//! `nullifier_hash` so the program can refuse a second withdrawal of it.
use pool::{Hash, Path, bind};
use xark::prelude::*;
use xark_poseidon2::hash;

/// A deposit's secret: whoever knows it can withdraw the deposit, once.
#[input]
pub struct Note {
    pub nullifier: Field,
    pub secret: Field,
}

#[input]
impl Note {
    /// The leaf a deposit appends: it hides both secrets.
    pub fn commitment(&self) -> Field {
        hash(self.to_fields())
    }

    /// What a withdrawal reveals: the same for every withdrawal of this note, and
    /// unlinkable to its commitment without the secret.
    pub fn nullifier_hash(&self) -> Field {
        hash([self.nullifier])
    }
}

/// What the program checks a withdrawal against, the public inputs in this order.
/// `recipient`, `relayer` and `fee` come from the transaction, so the proof only
/// pays out as it was made to.
#[input]
pub struct Withdrawal {
    pub root: Field,
    pub nullifier_hash: Field,
    pub recipient: Hash,
    pub relayer: Hash,
    pub fee: u64,
}

#[input]
impl Withdrawal {
    /// Withdrawing `note`, at `path`, to `recipient`, paying `fee` to `relayer`.
    pub fn of(note: Note, path: Path, recipient: Hash, relayer: Hash, fee: u64) -> Withdrawal {
        Withdrawal {
            root: path.root(note.commitment()),
            nullifier_hash: note.nullifier_hash(),
            recipient,
            relayer,
            fee,
        }
    }
}

#[circuit]
pub fn shielded_pool_withdraw(
    note: Private<Note>,
    path: Private<Path>,
    withdrawal: Public<Withdrawal>,
) {
    // The note is in the tree, and this is its nullifier hash.
    require_eq(path.root(note.commitment()), withdrawal.root);
    require_eq(note.nullifier_hash(), withdrawal.nullifier_hash);
    // Nothing else uses the addresses, so bind them: the proof pays only them.
    // (`fee` is a `u64`: its range check on entry already binds it.)
    bind(withdrawal.recipient);
    bind(withdrawal.relayer);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(n: u64) -> Note {
        Note {
            nullifier: Field::from(n),
            secret: Field::from(n + 1000),
        }
    }

    #[test]
    fn withdraws_a_note_in_the_tree() {
        let (mine, theirs) = (note(1), note(2));
        // I deposit, then they do: my path changes with their deposit.
        let path = Path::first();
        let tip = path.next(mine.commitment());
        let path = path.update(tip, theirs.commitment());
        let root = tip.next(theirs.commitment()).tree_root();

        let ok = Withdrawal::of(mine, path, [1; 32].into(), [2; 32].into(), 5);
        assert_eq!(ok.root, root);
        shielded_pool_withdraw(mine, path, ok).unwrap();
        // Another note at my path, a root my note isn't in, or another nullifier hash.
        let theirs_here = Withdrawal {
            nullifier_hash: theirs.nullifier_hash(),
            ..ok
        };
        assert!(shielded_pool_withdraw(theirs, path, theirs_here).is_err());
        let stale = Withdrawal {
            root: tip.tree_root(),
            ..ok
        };
        assert!(shielded_pool_withdraw(mine, path, stale).is_err());
        let nullifier_hash = theirs.nullifier_hash();
        assert!(
            shielded_pool_withdraw(
                mine,
                path,
                Withdrawal {
                    nullifier_hash,
                    ..ok
                }
            )
            .is_err()
        );
    }
}
