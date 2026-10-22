# 03 · Shielded pool: unlinkable transfers (Tornado-style)

Deposit a fixed amount, then withdraw it to a fresh address so that nobody can link the withdrawal
to the deposit. On-chain an observer sees a deposit and, later, an unrelated-looking withdrawal.
This is the first real privacy primitive in the ladder: it combines proofs ([01](../01-over-9000/))
and commitments ([02](../02-age-verification/)) with Merkle membership and nullifiers.

> **Reference implementation. Unaudited. Educational.** Mixers carry real legal and regulatory weight
> depending on where you are. Never use dev-mode keys for anything that holds value.

## How it works

Every deposit appends a **note commitment**, `Poseidon2(nullifier, secret)`, as a leaf of a Merkle
tree. To withdraw, you prove in zero knowledge that:

1. **membership**: your commitment is *some* leaf in the tree, without revealing which; and
2. **no double-withdraw**: you reveal `nullifier_hash = Poseidon2(nullifier)`, a value only the
   note's owner can produce, which the pool records so that note can never be spent twice.

Unlinkability comes from (1): you prove you own *a* leaf, not *which* leaf, so the withdrawal can't
be tied to any particular deposit. Your anonymity set is every deposit in the tree, which is why a
mixer is only as private as it is used.

### The chain never hashes

A shielded pool is a Merkle tree, and the classic trap is that the Poseidon in your circuit and the
Poseidon on-chain must be **byte-for-byte identical** or the roots never match. Instead of
reimplementing the hash in a Solana program, all hashing stays in the circuits:

- **`deposit`** proves *"`new_root` is the tree rooted at `old_root` with `leaf` appended at
  `index`"*. Public inputs: `old_root, new_root, leaf, index`.
- **`withdraw`** proves membership and derives the nullifier hash. Public inputs:
  `root, nullifier_hash, recipient, relayer, fee`.

The program stores only a ring of recent roots, the next index, and one marker account per spent
nullifier. It verifies proofs and moves lamports: no Poseidon on-chain, and circuit/chain agreement
holds *by construction*.

### One type for the whole tree: `Path`

A wallet tracks the tree as **the path of its next free leaf**. That path's root with the empty leaf
is the tree's root; its root with your note is the tree's root once your note is appended. So a
deposit is one `Path`, and its public statement is that path's append:

```rust
#[input]
pub struct Append { pub old_root: Field, pub new_root: Field, pub leaf: Field, pub index: u32 }

#[input]
impl Append {
    /// `leaf` appended at `path`, the next free leaf's path.
    pub fn of(path: Path, leaf: Field) -> Append {
        Append { old_root: path.tree_root(), new_root: path.root(leaf), leaf, index: path.index }
    }
}

#[circuit]
pub fn shielded_pool_deposit(path: Private<Path>, append: Public<Append>) {
    require_eq(Append::of(path, append.leaf), append);
}
```

Only the tree's true nodes reproduce `old_root` (the hash's collision resistance), so the depositor
can't smuggle in a different tree. A withdrawal proves your note's own `Path` against a root the
program remembers; its public statement is a `Withdrawal`. Each circuit's public inputs are one
`#[input]` struct, so the program's input order is a type, and the wallet builds it with the
circuit's own `Append::of` and `Withdrawal::of`.

Every root a circuit computes is `xark-merkle`'s `merkle_root`, whose fold is formally verified
(Lean, in xark). `Path` adds what a wallet needs around it.

`Path` lives in [`circuits/pool`](./circuits/pool/src/lib.rs), an ordinary Rust crate both circuits
import (and [04](../04-shielded-transfer/) reuses). Its `#[input] impl` puts the same code in the
TypeScript clients, so the wallet *is* the circuit's Rust:

```ts
let tip = Path.first();                        // the empty tree's next free leaf
let mine = tip;                                // my note lands here
tip = tip.next(Note.commitment(note));         // the tree after my deposit
// someone deposits `theirs`: my path changes beside it
mine = mine.update(tip, theirs); tip = tip.next(theirs);
withdrawals.prove({ note, path: mine, withdrawal: Withdrawal.of(note, mine, recipient, relayer, fee) });
```

`first`, `next` and `update` are host-only Rust (`#[cfg(not(xark))]`), and their tests check every
step against a tree hashed from scratch. The empty subtrees they need are hashed on the host; the
circuits never need them.

**Hash domains.** Tree nodes are Poseidon (`xark-merkle`'s); commitments and nullifier hashes are
Poseidon2, a different permutation, whose sponge also seeds its capacity with the input length (2
and 1). No node, commitment or nullifier hash can collide with another kind.

**Front-run protection.** `recipient`, `relayer` and `fee` are public inputs the program fills from
the transaction, so a relayer can't rewrite where your withdrawal goes. A Solana address is wider
than the BN254 field, so it enters as xark's `Hash` type, two 16-byte halves; the client takes its
32 bytes. One subtlety: Groth16 binds a public input only through the constraints that use it, and
`xark build` refuses one that none use. `fee` is a `u64`, so its range check on entry already uses
it; the two addresses get one constraint per half (`pool::bind`, Tornado's `recipientSquare`).

**Multi-denomination.** Each denomination is its own pool: `initialize(denomination)` creates a PDA
seeded by the amount, like Tornado's separate 0.1 / 1 / 10 pools. One program serves every size, and
nullifiers are namespaced per denomination. Amounts are hidden *within* a pool because every note in
it has identical value; arbitrary hidden amounts need value-carrying notes, which is
[04](../04-shielded-transfer/).

## Anatomy

```
circuits/pool/       Path (the tree, for circuits and wallets), bind: shared with 04
circuits/deposit/    proves a tree append                (public: Append)
circuits/withdraw/   proves membership + nullifier, and  (public: Withdrawal)
                     Note's commitment and nullifier hash
client/              both generated clients, and the wallet test
program/             Anchor: verify both proofs, hold and pay lamports, track roots and nullifiers
```

## Run it

```bash
just build          # both circuits → dev keys → verifier crates → clients → .so
just test-circuit   # the tree's tests, then each circuit's
cd .. && npx vitest run 03
```

The [test](./client/shielded-pool.test.ts) is the reference wallet and client: two deposits, a path
kept current across the second, a withdrawal relayed to a fresh address for a fee, and the refusals
(another recipient, another fee, an unknown root, the same note twice). It also checks the
program's empty root against `Path.first().treeRoot()`.

## What it gives you, and doesn't

**Gives you:** unlinkability between deposit and withdrawal across the anonymity set; double-spend
protection via one-time nullifiers; front-run protection from binding recipient, relayer and fee
into the proof.

**Doesn't:** hide amounts across denominations (fixed size per pool); resist timing or amount
correlation if the set is tiny or you withdraw instantly; offer a compliance layer (viewing keys,
association sets are out of scope); or carry an audit.

**Submission model.** A deposit proof binds the current root and next index. Clients must sequence
deposits, read the pool's state just before proving, and prove again if another deposit lands
first. A high-throughput deployment needs a sequencer/batcher or a different append design.

## Reference

### Instructions

**`initialize(denomination: u64)`**: create the pool for a denomination (the empty-tree root is
fixed on-chain). Accounts: `pool` (init, PDA), `authority` (signer, payer), `system_program`.

**`deposit(commitment: [u8;32], new_root: [u8;32], proof: [u8;256])`**: deposit the denomination and
append `commitment`; `proof` attests that `new_root` extends the tree by one leaf. Accounts: `pool`
(mut), `depositor` (signer), `system_program`. Public inputs: `Append` (`old_root, new_root, leaf,
index`).

**`withdraw(proof: [u8;256], root: [u8;32], nullifier_hash: [u8;32], fee: u64)`**: pay
`denomination − fee` to `recipient` and `fee` to the relayer; the `nullifier` PDA is created here, so
a repeat with the same nullifier fails. Accounts: `pool` (mut), `nullifier` (init, PDA), `recipient`
(unchecked, mut), `relayer` (signer, payer), `system_program`. Public inputs:
`Withdrawal` (`root, nullifier_hash, recipient (hi, lo), relayer (hi, lo), fee`).

### PDAs & state

| PDA | Seeds |
|---|---|
| `pool` | `["pool", denomination as u64 LE]` |
| `nullifier` | `["nullifier", denomination as u64 LE, nullifier_hash]` |

`Pool` state: `denomination: u64`, `next_index: u32`, `current_root_index: u32`,
`roots: [[u8;32]; 16]` (the recent-root ring), `bump: u8`. An address enters the circuit as a
`Hash`, `[hi, lo]`: each 16-byte half read as a big-endian integer, as a little-endian field
element.

### Errors

| Error | Message |
|---|---|
| `InvalidProof` | invalid proof |
| `UnknownRoot` | unknown or stale merkle root |
| `FeeTooHigh` | fee exceeds denomination |
| `TreeFull` | tree is full |

Next → [04 · Shielded transfer](../04-shielded-transfer/)
