//! Shielded transfer: the JoinSplit `transact` circuit, 2 notes in, 2 out.
//!
//! One circuit is deposit, private transfer and withdrawal: `vpub_in` is value
//! entering the pool (a deposit), `vpub_out` value leaving it (a withdrawal), and a
//! transfer sets both to 0. A spend can be a dummy (`real: false`, worth 0, no
//! path), so a deposit spends nothing.
//!
//! It proves, in one proof:
//! - each real note spent is in the tree at `root`, its spender holds its key, and
//!   its nullifier is the one revealed (so it can't be spent again);
//! - the two new notes are the `commitments` revealed, and are appended as a pair at
//!   `index`, taking the tree from `old_root` to `new_root` (03's deposit, for two);
//! - value is conserved: `in + vpub_in == out + vpub_out + fee`.
//!
//! Every value is a `u64` input, range-checked on entry, so the sums can't wrap.
//!
//! Notes reach their owners off-circuit: the sender publishes an encrypted memo per
//! note and owners trial-decrypt. Each memo's hash is a public input, so a submitter
//! can't swap a memo without breaking the proof.
//!
//! The public inputs are one [`Transaction`].
use pool::{Hash, Path, bind, halves};
use xark::prelude::*;
use xark_poseidon2::hash;

// Domain tags, on top of each hash's length tag: no two kinds of value collide.
const NULLIFIER_KEY: u64 = 1;
const VIEWING_KEY: u64 = 2;
const ADDRESS: u64 = 3;
const COMMITMENT: u64 = 4;
const NULLIFIER: u64 = 5;

/// A holder's spending key: their other keys and every address derive from it.
#[input]
pub struct Key {
    pub secret: Field,
}

#[input]
impl Key {
    /// What nullifiers are made with: whoever has it sees when the holder spends.
    pub fn nullifier_key(&self) -> Field {
        hash([self.secret, Field::from(NULLIFIER_KEY)])
    }

    /// What addresses are made from: whoever has it recognizes the holder's addresses.
    pub fn viewing_key(&self) -> Field {
        hash([self.secret, Field::from(VIEWING_KEY)])
    }

    /// The holder's address number `diversifier`: any number of them, unlinkable to
    /// each other without the viewing key.
    pub fn address(&self, diversifier: Field) -> Field {
        hash([self.viewing_key(), diversifier, Field::from(ADDRESS)])
    }
}

/// `value` of the pool's token, owned by `owner` (a [`Key::address`]). `rho` and
/// `rseed` are the sender's randomness: they make every note unique and hidden.
#[input]
pub struct Note {
    pub value: u64,
    pub owner: Field,
    pub rho: Field,
    pub rseed: Field,
}

#[input]
impl Note {
    /// The leaf the note is in the tree as, in the pool of `asset` (its mint).
    pub fn commitment(&self, asset: Hash) -> Field {
        let [hi, lo] = halves(asset);
        let [value, owner, rho, rseed] = self.to_fields();
        hash([value, owner, rho, rseed, hi, lo, Field::from(COMMITMENT)])
    }

    /// What spending the note at `position` reveals: only `key`'s holder can make
    /// it, and it's the same each time, so the program refuses a second spend.
    pub fn nullifier(&self, key: Key, asset: Hash, position: u32) -> Field {
        nullifier(key.nullifier_key(), self.commitment(asset), position)
    }
}

fn nullifier(nullifier_key: Field, commitment: Field, position: u32) -> Field {
    hash([
        nullifier_key,
        commitment,
        Field::from(position),
        Field::from(NULLIFIER),
    ])
}

/// A note to spend, at `path`, owned by the spender's address number
/// `diversifier`. A dummy (`real: false`) is worth 0 and its path isn't checked.
#[input]
pub struct Spend {
    pub note: Note,
    pub diversifier: Field,
    pub path: Path,
    pub real: bool,
}

/// The value a transaction moves in and out of the pool: `vpub_in` in from the
/// user's token account, `vpub_out` out to `recipient`, and `fee` to `relayer`, the
/// token account of whoever submits it (so the spender needs no SOL, and signs
/// nothing that names them).
#[input]
pub struct Flow {
    pub vpub_in: u64,
    pub vpub_out: u64,
    pub fee: u64,
    pub recipient: Hash,
    pub relayer: Hash,
}

/// What the program checks a transaction against, the public inputs in this order:
/// the root the spends are in, the pool's mint, what the spends reveal and the
/// outputs append, the tree's root before and after, the flow, and the memos'
/// SHA-256 hashes.
#[input]
pub struct Transaction {
    pub root: Field,
    pub asset: Hash,
    pub nullifiers: [Field; 2],
    pub commitments: [Field; 2],
    pub old_root: Field,
    pub new_root: Field,
    pub index: u32,
    pub flow: Flow,
    pub memos: [Hash; 2],
}

#[input]
impl Transaction {
    /// `key` spends `spends` and creates `outputs`, appended at `tip` (the next free
    /// leaf's path) in the pool of `asset`.
    #[cfg(not(xark))]
    pub fn of(
        key: Key,
        spends: [Spend; 2],
        outputs: [Note; 2],
        tip: Path,
        asset: Hash,
        flow: Flow,
        memos: [Hash; 2],
    ) -> Transaction {
        let commitments = outputs.map(|o| o.commitment(asset));
        Transaction {
            root: tip.tree_root(),
            asset,
            nullifiers: spends.map(|s| s.note.nullifier(key, asset, s.path.index)),
            commitments,
            old_root: tip.tree_root(),
            new_root: pair_root(tip, commitments),
            index: tip.index,
            flow,
            memos,
        }
    }
}

/// The root once `pair` is appended at `tip`, the next free leaf's path, at an even
/// index: the second leaf's path is `tip` with the first beside it.
fn pair_root(tip: Path, pair: [Field; 2]) -> Field {
    let mut siblings = tip.siblings;
    siblings[0] = pair[0];
    let second = Path {
        index: tip.index + 1,
        siblings,
    };
    second.root(pair[1])
}

/// `tip` is the next free leaf's path: the outputs land there and after it.
#[circuit]
pub fn shielded_transact(
    key: Private<Key>,
    spends: Private<[Spend; 2]>,
    outputs: Private<[Note; 2]>,
    tip: Private<Path>,
    tx: Public<Transaction>,
) {
    let nullifier_key = key.nullifier_key();

    // Spend: each note is the spender's and burns a nullifier; a real one is in the
    // tree, and a dummy is worth nothing.
    let mut value_in = Field::from(tx.flow.vpub_in);
    for (spend, revealed) in spends.into_iter().zip(tx.nullifiers) {
        require_eq(spend.note.owner, key.address(spend.diversifier));
        let commitment = spend.note.commitment(tx.asset);
        if spend.real {
            require_eq(spend.path.root(commitment), tx.root);
        } else {
            require(spend.note.value == 0);
        }
        require_eq(
            nullifier(nullifier_key, commitment, spend.path.index),
            revealed,
        );
        value_in += Field::from(spend.note.value);
    }

    // Create: the outputs are the commitments revealed.
    let mut value_out = Field::from(tx.flow.vpub_out) + Field::from(tx.flow.fee);
    for (output, revealed) in outputs.into_iter().zip(tx.commitments) {
        require_eq(output.commitment(tx.asset), revealed);
        value_out += Field::from(output.value);
    }
    // Every value is a range-checked `u64`, so these few sums can't wrap the field.
    require_eq(value_in, value_out);

    // Append the pair at `index`, an even slot. As in 03's deposit, the old root
    // pins `tip` to the tree's true nodes.
    require(tip.index == tx.index);
    require(tx.index.is_multiple_of(2));
    require_eq(tip.tree_root(), tx.old_root);
    require_eq(pair_root(tip, tx.commitments), tx.new_root);

    // Where the value and the fee go, and the memos, are the ones the proof was made for.
    bind(tx.flow.recipient);
    bind(tx.flow.relayer);
    for memo in tx.memos {
        bind(memo);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xark::prover::ProveError;

    fn asset() -> Hash {
        [7; 32].into()
    }

    fn note(key: Key, value: u64, rho: u64) -> Note {
        Note {
            value,
            owner: key.address(Field::from(1u64)),
            rho: Field::from(rho),
            rseed: Field::from(rho + 1),
        }
    }

    fn spend(note: Note, path: Path) -> Spend {
        Spend {
            note,
            diversifier: Field::from(1u64),
            path,
            real: true,
        }
    }

    fn dummy(key: Key, rho: u64) -> Spend {
        Spend {
            real: false,
            ..spend(note(key, 0, rho), Path::first())
        }
    }

    /// Prove `spends` → `outputs` with the tree's next free leaf at `tip`.
    fn transact(
        key: Key,
        spends: [Spend; 2],
        outputs: [Note; 2],
        tip: Path,
        vpub: (u64, u64, u64),
    ) -> Result<(), ProveError> {
        let flow = Flow {
            vpub_in: vpub.0,
            vpub_out: vpub.1,
            fee: vpub.2,
            recipient: [9; 32].into(),
            relayer: [8; 32].into(),
        };
        let memos = [[1; 32].into(), [2; 32].into()];
        let tx = Transaction::of(key, spends, outputs, tip, asset(), flow, memos);
        // At an even index, the only kind the circuit takes, the circuit's pair
        // append agrees with the wallet's.
        if tip.index.is_multiple_of(2) {
            let [first, second] = tx.commitments;
            assert_eq!(tx.new_root, tip.next(first).next(second).tree_root());
        }
        shielded_transact(key, spends, outputs, tip, tx)
    }

    #[test]
    fn deposit_then_pay_privately() {
        let (alice, bob) = (
            Key {
                secret: Field::from(42u64),
            },
            Key {
                secret: Field::from(99u64),
            },
        );
        let dummies = [dummy(alice, 100), dummy(alice, 101)];

        // Alice deposits 150: nothing spent, 150 in.
        let mine = note(alice, 150, 1);
        let deposit = [mine, note(alice, 0, 2)];
        transact(alice, dummies, deposit, Path::first(), (150, 0, 0)).unwrap();
        assert!(
            transact(
                alice,
                dummies,
                [note(alice, 151, 1), deposit[1]],
                Path::first(),
                (150, 0, 0)
            )
            .is_err()
        );

        // Then pays Bob 100 of it, 50 back as change, with no value in or out.
        // Her note's path changed when the deposit's second output landed beside it.
        let a = asset();
        let after_mine = Path::first().next(mine.commitment(a));
        let path = Path::first().update(after_mine, deposit[1].commitment(a));
        let tip = after_mine.next(deposit[1].commitment(a));
        let pay = [note(bob, 100, 3), note(alice, 50, 4)];
        let spends = [spend(mine, path), dummy(alice, 102)];
        transact(alice, spends, pay, tip, (0, 0, 0)).unwrap();

        // Bob can't spend Alice's note; a dummy can't carry value; nor create it.
        assert!(
            transact(
                bob,
                [spend(mine, path), dummy(bob, 102)],
                pay,
                tip,
                (0, 0, 0)
            )
            .is_err()
        );
        let worth = Spend {
            real: false,
            ..spends[0]
        };
        assert!(transact(alice, [worth, dummy(alice, 102)], pay, tip, (0, 0, 0)).is_err());
        assert!(transact(alice, spends, [note(bob, 101, 3), pay[1]], tip, (0, 0, 0)).is_err());
        // Outputs land in pairs, at an even index.
        assert!(transact(alice, spends, pay, tip.next(Field::from(5u64)), (0, 0, 0)).is_err());

        // Relayed for a fee of 5, paid out of the notes spent: 150 = 100 + 45 + 5.
        let relayed = [note(bob, 100, 3), note(alice, 45, 4)];
        transact(alice, spends, relayed, tip, (0, 0, 5)).unwrap();
        assert!(transact(alice, spends, pay, tip, (0, 0, 5)).is_err());
    }
}
