# 04 · Shielded pool: unlinkable transfers (Tornado-style)

Deposit a fixed amount, then withdraw it to a fresh address so that nobody can link the withdrawal
to the deposit. Onchain, an observer sees a deposit and, later, an unrelated-looking withdrawal.
This is the first real privacy primitive in the ladder: [03](../03-token-claim/) hid the path but
not who claimed, and a nullifier hides that too. It's [03](../03-token-claim/)'s **Merkle tree**
with [02](../02-zkyc/)'s **salted commitments** as its leaves, plus nullifiers, in a tree that grows
with every deposit. Range proofs play only a supporting role: they bound the relayer's `fee` (a
`u64`, which the program also checks is at most the denomination) and the leaf index (a `u32`,
bounded below 2^20 as its bits steer the path). Amounts need none, since every note in a pool is the
same denomination; [05](../05-shielded-transfer/) is where range proofs become essential.

> **Reference implementation. Unaudited. Educational.** Mixers carry real legal and regulatory weight
> depending on where you are. For anything that holds value, use keys from a multi-party
> ceremony (`xark ceremony`), so no one has to trust whoever ran setup.

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
Poseidon onchain must be **byte-for-byte identical** or the roots never match. Instead of
reimplementing the hash in a Solana program, all hashing stays in the circuits:

- **`deposit`** proves *"`new_root` is the tree rooted at `old_root` with `leaf` appended at
  `index`"*. Public inputs: `old_root, new_root, leaf, index`.
- **`withdraw`** proves membership and derives the nullifier hash. Public inputs:
  `root, nullifier_hash, recipient, relayer, fee`.

The program stores only a ring of recent roots, the next index, and one marker account per spent
nullifier. It verifies proofs and moves lamports: no Poseidon onchain, and circuit/chain agreement
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
import (and [05](../05-shielded-transfer/) reuses). Its `#[input] impl` puts the same code in the
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
[05](../05-shielded-transfer/).

## Anatomy

```
circuits/pool/       Path (the tree, for circuits and wallets), bind: shared with 05
circuits/deposit/    proves a tree append                (public: Append)
circuits/withdraw/   proves membership + nullifier, and  (public: Withdrawal)
                     Note's commitment and nullifier hash
client/              both generated clients, and the wallet test
program/             Pinocchio: verify both proofs, hold and pay lamports, track roots and nullifiers
```

## Run it

```bash
npm run build          # both circuits → keys → verifier crates → clients → .so
npm run test:circuit   # the tree's tests, each circuit's, then the program's
npm test               # deposit, withdraw through a relayer, onchain (LiteSVM)
```

The [test](./client/shielded-pool.test.ts) is the reference wallet and client: two deposits, a path
kept current across the second, a withdrawal relayed to a fresh address for a fee, and the refusals
(another recipient, another fee, an unknown root, the same note twice). It also checks the
program's empty root against `Path.first().treeRoot()`.

The program's own [tests](./program/src/tests.rs) run on the host, against accounts laid out as the
runtime lays them out: each account check below refuses what it should, and the demo proofs verify
against the public inputs the program assembles, and not for another recipient, relayer or fee.

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

The program is [Pinocchio](https://github.com/anza-xyz/pinocchio), `no_std`:

```
program/src/
  lib.rs              the entrypoint, which dispatches on the first byte
  instructions/       initialize.rs, deposit.rs, withdraw.rs: one file per instruction
  state.rs            the pool's layout and checks, the nullifier marker, the seeds
  errors.rs           ShieldedPoolError
  helpers.rs          the shared account checks and logs
  tests.rs            host tests
```

Each instruction is an accounts struct and an instruction-data struct, each parsed with
`TryFrom`, and a `process` that verifies the proof and then moves state and lamports. Instruction
data is a one-byte discriminator, then the arguments' bytes in order; integers are little-endian,
and data of any other length is refused. The program's address is `ID` in `lib.rs`,
`Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS`, which the client test deploys it at; the
checks of the pool's and nullifiers' owner and addresses are against it.

### Instructions

**`0` `initialize(denomination: u64)`**, data `[0] ‖ denomination (8)`: create the pool for a
denomination, with the empty-tree root fixed onchain. Accounts: `pool` (writable, the PDA, not yet
created), `authority` (writable signer, pays the rent), the system program.

**`1` `deposit(commitment, new_root, proof)`**, data `[1] ‖ commitment (32) ‖ new_root (32) ‖ proof
(256)`: deposit the denomination and append `commitment`; `proof` attests that `new_root` extends
the tree by one leaf. Accounts: `pool` (writable), `depositor` (writable signer), the system
program. Public inputs: `Append` (`old_root, new_root, leaf, index`). Logs `Program data:` with the
commitment and its leaf index (u32), for a wallet to follow the tree.

**`2` `withdraw(proof, root, nullifier_hash, fee: u64)`**, data `[2] ‖ proof (256) ‖ root (32) ‖
nullifier_hash (32) ‖ fee (8)`: pay `denomination − fee` to `recipient` and `fee` to the relayer;
the `nullifier` PDA is created here, so a repeat with the same nullifier fails. Accounts: `pool`
(writable), `nullifier` (writable, the PDA, not yet created), `recipient` (writable, any account but
the pool's own two), `relayer` (writable signer, pays the rent), the system program. Public inputs:
`Withdrawal` (`root, nullifier_hash, recipient (hi, lo), relayer (hi, lo), fee`).

### Account checks

Every account check is written out, in each instruction's `TryFrom` (with the shared ones in
`helpers.rs` and `Pool::check`), before `process` runs:

- signers and payers sign and are writable; every account the program writes to is writable;
- the system program is the system program;
- the pool is owned by the program, is exactly a pool's size with a pool's tag, and sits at the
  address its own denomination and stored bump derive (one hash, no bump search);
- a new pool or nullifier sits at the address its seeds derive with the canonical bump (the bump
  `find_program_address` returns), so there is exactly one per denomination, and one per nullifier;
- a new account must not exist yet: owned by the system program with no data. Lamports sent to the
  address beforehand don't block it; creation tops them up to rent exemption;
- a nullifier that exists is spent (`NullifierSpent`);
- the lamport moves are checked arithmetic, and the runtime keeps the pool rent-exempt.

### PDAs & state

| PDA | Seeds | Size |
|---|---|--:|
| `pool` | `["pool", denomination as u64 LE]` | 536 |
| `nullifier` | `["nullifier", denomination as u64 LE, nullifier_hash]` | 1 |

The pool's layout, every field little-endian:

| Offset | Size | Field |
|--:|--:|---|
| 0 | 1 | tag, `1` |
| 1 | 1 | bump |
| 2 | 6 | reserved, zero |
| 8 | 8 | `denomination: u64` |
| 16 | 4 | `next_index: u32` |
| 20 | 4 | `current_root_index: u32` |
| 24 | 512 | `roots: [[u8; 32]; 16]`, the recent-root ring |

A nullifier's account is one byte, its tag, `2`; that it exists is what counts. An address enters
the circuit as a `Hash`, `[hi, lo]`: each 16-byte half read as a big-endian integer, as a
little-endian field element.

### Errors

The program's own errors are `ShieldedPoolError`, `custom program error` codes from 0, and each
logs its name and message. Everything else fails with a built-in `ProgramError`:
`NotEnoughAccountKeys`, `InvalidInstructionData`, `MissingRequiredSignature`, `Immutable`,
`IncorrectProgramId`, `InvalidAccountOwner`, `InvalidAccountData`, `InvalidSeeds`,
`AccountAlreadyInitialized`, `InvalidArgument` (a payout to the pool's own accounts),
`InsufficientFunds` and `ArithmeticOverflow`.

| Code | Error | Message |
|--:|---|---|
| 0 | `InvalidProof` | invalid proof |
| 1 | `UnknownRoot` | unknown or stale merkle root |
| 2 | `FeeTooHigh` | fee exceeds denomination |
| 3 | `TreeFull` | tree is full |
| 4 | `NullifierSpent` | nullifier already spent |

### Numbers

| Instruction | Onchain CU |
|---|--:|
| `initialize` | 2,988 |
| `deposit` | 91,154 to 91,747 |
| `withdraw` | 110,351 to 113,351 |

Measured in the client test's LiteSVM, the whole instruction with its system-program CPI. A deposit
varies with its public inputs; a withdrawal also with how many bumps the nullifier's address takes
to find (1,500 CU each).

Next → [05 · Shielded transfer](../05-shielded-transfer/)
