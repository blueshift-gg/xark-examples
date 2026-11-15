# 05 · Anonymous voting: one person, one vote, no names

A poll's authority publishes one 32-byte root for a list of registered voters. Each voter proves
they're on the list and casts one vote, and the program counts it. Nobody learns who voted for
what, or whether two polls' votes came from the same person. [04](../04-shielded-pool/) put salted
commitments in a Merkle tree and spent each one once with a nullifier. This is that pool with
nothing to withdraw, and with one new idea: **a scoped nullifier**. A note in 04 has one nullifier
for its whole life; a voter here has a different one in every poll. Within a poll, a second vote
reuses the nullifier and is refused. Across polls, two nullifiers of one voter look unrelated.
Range proofs come back too, smaller than in 01: the choice is proved below the poll's option count.

> **Educational reference, not an audited product.** It uses a single-party setup key; for a vote
> that matters, use keys from a multi-party ceremony (`xark ceremony`).

## A voter is two secrets

A voter picks two random field elements and registers a commitment to them:

```
identity   = (nullifier_key, trapdoor)       kept by the voter
commitment = Poseidon2.commit([nullifier_key, trapdoor])       on the voter list, a leaf
nullifier  = Poseidon2.nullifier([nullifier_key, poll])        revealed by a vote in `poll`
```

The list is 03's tree, `xark-merkle`'s Poseidon2 tree: each commitment is a leaf, the leaves in
order, padded with an empty leaf to 2^20, and each node is `hash2(left, right)`. The authority sees
commitments, and the chain sees nullifiers. Neither says anything about the other without the
`nullifier_key`.

There are three hashes, each in its own domain of Poseidon2 (`xark-poseidon2`). `commit` and
`nullifier` are the sponge with its capacity lane started at `2^64 · 1 + 2` and `2^64 · 2 + 2` (the
domain, then the input's length), and a node is the compression `hash2`, one permutation of
`[a, b, 0]`, capacity 0. So no node is a voter's commitment, no nullifier is either, and with the
depth fixed at 20, a node can't be passed off as a voter at a shallower one. The empty leaf is
ASCII `xarkzero`, a constant that no commitment hashes to, so the padding can't vote.

## The circuit

```rust
#[input]
pub struct Identity { pub nullifier_key: Field, pub trapdoor: Field }

#[input]
impl Identity {
    pub fn commitment(&self) -> Field { commit([self.nullifier_key, self.trapdoor]) }
    pub fn nullifier(&self, poll: Field) -> Field { nullifier([self.nullifier_key, poll]) }
}

/// What the program checks a vote against, the public inputs in this order.
#[input]
pub struct Ballot {
    pub root: Field,
    pub poll: Field,
    pub nullifier: Field,
    pub options: u8,
    pub choice: u8,
}

#[circuit]
pub fn vote(identity: Private<Identity>, path: Private<Path>, ballot: Public<Ballot>) {
    require_eq(path.root(identity.commitment()), ballot.root);
    require_eq(identity.nullifier(ballot.poll), ballot.nullifier);
    require(ballot.choice < ballot.options);
}
```

`Path` is 03's, line for line: a leaf's index and its 20 siblings, folded with `xark-merkle`'s
Poseidon2 `merkle_root` (a `cond_swap` and a `hash2` per level). An authority in Rust builds the
list with the crate's `list()`, a `xark_merkle::tree::Tree`.

- **Private:** the `Identity`, and the `Path` to its commitment: the 20 siblings and the leaf's
  index. The index says which voter this is, so it never leaves the voter's device.
- **Public:** the `Ballot`: `root`, `poll`, `nullifier`, then `options` and `choice`. `#[input]`
  packs consecutive integers into one field element (`options` in bits 0–7, `choice` in 8–15), so
  that's four public inputs. The generated verifier crate packs them the same way: the program
  names the fields and never shifts a bit.
- **No `bind`.** `root` and `nullifier` are compared, `poll` is hashed into the nullifier, and
  `options` and `choice` are `u8`s, range-checked on entry and then ordered. Every public input is
  already in the constraints.

### The range proof

`choice < options` is 01's range proof, between two values rather than against a constant. Each
`u8` is decomposed into 8 bits on entry, so both are below 2^8. Then xark's comparison uses a
borrow bit `lt`, which the prover supplies, and range-proves `choice − options + lt · 2^8` in 8 bits.
With `lt = 1` (the `require`), that's `choice − options + 256 ∈ [0, 256)`, true exactly when
`choice < options`. With `choice ≥ options` there's no 8-bit value to prove, and a "negative"
difference wraps to a number near the field's size, nowhere near 8 bits. That's 27 constraints: 16 for
the two inputs' bits, 10 for the comparison and 1 for the `require`.

Without the bits, nothing stops a prover from voting for option `p − 1`, which in the field is
"less than" anything. The circuit has no idea of a tally. The bits do.

**Why `options` is a public input.** The other design fixes a maximum in the circuit (`choice <
8u8`) and leaves the program to check `choice < options`. Making `options` public puts the whole
statement in the proof, and lets one circuit, one key and one verifier serve polls of any size. The
program supplies `options` from the poll's account, as it does `root` and `poll`, and never checks
the choice itself. The packing makes it free: `options` shares a public input with `choice`.

## The scoped nullifier

The nullifier is `nullifier([nullifier_key, poll])`, the voter's secret and the poll's public ID.

- **One vote per poll.** In one poll, a voter's nullifier is always the same: a second vote, with a
  fresh proof, reveals the same nullifier, and the program finds its receipt.
- **Unlinkable across polls.** In another poll it's another hash of the same key. Telling whether
  two nullifiers share a key means knowing the key (or breaking Poseidon2). Voting in ten polls
  leaves ten unrelated nullifiers.
- **Unlinkable to the list.** The commitment is a different function of the key, and of the
  trapdoor, which no nullifier involves. The authority, which knows every commitment, can't match a
  nullifier to one.

So the poll ID must be unique, or two polls would share each voter's nullifier. The poll's PDA is
seeded by its `u64` ID alone, `["poll", id]`, not by the authority as well. Two authorities can't
both make poll 7, so no two polls share an ID, or a voter's nullifier.

## Who submits the vote

The proof doesn't name the signer, and that's deliberate. 03 bound the claimant: the program took
it from the signer, so only the claimant's wallet could claim. A vote must not carry the voter's
wallet, or the wallet is the name. So any signer pays for the receipt and the fee, and the test
votes through a relayer.

That's safe because the relayer can't change the vote. `poll`, `options` and `root` come from the
poll's account; `nullifier` and `choice` from the instruction. All of them are public inputs of the
proof. A relayer that changes the choice, or sends the proof to another poll, has a proof that
doesn't verify. All it can do is drop the vote, and the voter then submits elsewhere. A Groth16
proof can be re-randomized, but the copy proves the same ballot, and the nullifier's receipt counts
it once.

**A voter should submit through a relayer or a fresh wallet.** Signing with a known wallet doesn't
break the proof. It just puts your name beside your vote, the one thing the circuit was keeping out.

## What it doesn't hide

- **The tally.** Each vote's choice is in its transaction, and the running count is public at
  every slot. Only who made it is hidden. Hiding the choices until the poll closes needs encrypted
  ballots (homomorphic tallying, or commit-and-reveal), out of scope here.
- **The timing.** Every vote is a transaction at a known slot. If only one person on the list was
  awake at 3 a.m., a vote at 3 a.m. is theirs. Small lists and quiet hours shrink the anonymity set,
  which is the list, and only as large as the voters who act like everyone else.
- **The relayer's view.** A relayer sees the network address the vote came from, unless the voter
  hides that too.
- **Who's registered.** The list is the authority's. It knows who each commitment belongs to, and
  could register fake voters. The circuit proves membership in the list, not that the list is fair.

## The program

The [program](./programs/anonymous-voting/src/lib.rs) is Pinocchio: `lib.rs` dispatches on each
instruction's discriminator to [`instructions/`](./programs/anonymous-voting/src/instructions/), and
the poll's layout is in [`state.rs`](./programs/anonymous-voting/src/state.rs). `create_poll` stores
the root, the option count and an empty tally. `vote` assembles the public inputs itself, by name,
and makes one call to the verifier crate `xark build` generated
([`clients/vote/rust`](./clients/vote/rust/src/lib.rs)), which packs them as the circuit does:

```rust
pub fn public_inputs(poll: &Poll, nullifier: &[u8; 32], choice: u8) -> PublicInputs {
    PublicInputs {
        ballot: Ballot {
            root: poll.root,
            poll: fields::uint(u64::from_le_bytes(poll.id).into()),
            nullifier: *nullifier,
            options: poll.options,
            choice,
        },
    }
}

if !verify(&Proof::from_le_bytes(self.data.proof), inputs) {
    return Err(VotingError::InvalidProof.into());
}
```

Then it creates the nullifier's receipt, `["voted", poll, nullifier]`, and adds one to
`tally[choice]`, with checked arithmetic. The receipt sits at the canonical bump, the one
`find_program_address` returns. A receipt at another bump would be another address for the same
nullifier, and the voter could vote once at each. The verifier refuses a public input that isn't a
canonical field element, so `nullifier + r` can't pass for `nullifier` at another address either.
Lamports sent to a receipt's address beforehand don't block a vote: creation tops them up. The
optional `close_poll` lets the authority freeze the tally.

The program's own [tests](./programs/anonymous-voting/src/tests.rs) run on the host, against accounts
laid out as the runtime lays them out. They check that the demo proof
([`circuits/vote/examples/demo.rs`](./circuits/vote/examples/demo.rs)) verifies against
what the program assembles, and not for another choice, option count, poll, root or nullifier. They
also check that each account check refuses what it should.

## Run it

```bash
xark build     # the circuit, its clients (Rust verifier crate, TypeScript), then the program
xark test      # the circuit's tests (the builder against the Tree, who votes, what's
               # refused), the program's, then prove and vote onchain through a relayer (LiteSVM)
```

`npm run check` is CI's `xark build --check`: the committed keys and clients are what the source
builds. The keys in [`keys/vote/`](./keys/vote/) are a single-party setup from one machine's
randomness (see [../RUNBOOK.md](../RUNBOOK.md)). The verifier crate bundles a sample proof as its
self-test, made by `examples/demo.rs`; for a new key, run `xark setup`, then `cargo run --release
-p vote --example demo` before `xark build`.

The [test](./tests/anonymous-voting.test.ts) is the authority, five voters and a relayer. The
authority builds the list with the circuit's own `Identity.commitment` and `Path`, and creates a
three-option poll. Three voters vote through the relayer, and the tally reads `[1, 0, 2]`. Then
the program refuses each of these:

- voter 0 voting again, with a fresh proof (`AlreadyVoted`);
- voter 3's proof replayed for another choice, in range or out (`InvalidProof`);
- a proof made against four options (`InvalidProof`, since the program supplies three);
- the same proof sent to a second poll over the same list (`InvalidProof`).

Voter 0 votes in the second poll, with a nullifier unlike their first, and it counts. Once the
authority closes the second poll, a vote there fails with `PollClosed`. Two ballots have no proof at
all, and the client throws `CheckFailed`, naming the failing line: a choice of 3 in a three-option
poll (`ballot.choice < ballot.options`), and an identity not on the list
(`path.root(identity.commitment())`).

## Numbers

| Circuit | Constraints | Public inputs | Prove, 1 thread | Prove, 8 workers | Onchain CU |
|---|--:|--:|--:|--:|--:|
| `vote` | 5,401 | 4 | 947 ms | 191 ms | 90,229–100,635 |

Constraints from `xark build`; `xark profile` attributes them:

- 4,860 to the path (243 per level: a `hash2`, a `cond_swap` and an index bit);
- 240 each to the commitment and the nullifier, one permutation each;
- 32 to the `u32` index, 27 to the range proof, and 2 to the two `require_eq`s.

Proving in node 22 through the generated client, median of five, on an Apple M3 Max; "8 workers" is
`@blueshift-gg/xark`'s worker pool. Compute units are the whole `vote` in LiteSVM: the verify, the
receipt it creates and the tally. They vary with the nullifier, since the verifier's cost for a public input
depends on its value, and by 1,500 for each extra try of `find_program_address` for the receipt's
bump. A proof is always 256 bytes.

## Anatomy

```
circuits/vote/               Identity (commitment, nullifier), Path (the list, for the circuit and
                             its builder), Ballot, the circuit, and the demo proof that's the
                             verifier crate's self-test
programs/anonymous-voting/   Pinocchio: create a poll, verify a vote, write its receipt, count it;
                             close the poll
clients/vote/                generated by `xark build`: the verifier crate (rust/), the client (ts/)
keys/vote/                   the proving and verifying keys the clients embed
tests/                       the authority, voter and relayer test
```

## Deployment boundaries

- **The list is fixed per poll.** A poll stores one root. Registering later means a new list and a
  new poll; a voter's identity carries over, and their nullifiers stay unlinkable.
- **The poll ID is first come, first served.** Anyone can take an ID before an authority does. An
  authority should pick a random `u64`, and announce the poll's address with its root.
- **One program.** The nullifier hashes the poll's ID, not the program's address. Another
  deployment of this program could reuse an ID, so a voter on both lists would reveal the same
  nullifier in both.
- **Up to 8 options.** The tally is fixed at 8; the circuit takes up to 255.
- **Dev trusted setup.** A real deployment needs `xark ceremony`.

## Reference

### Instructions

Instruction data starts with one byte, which instruction; integers are little-endian, and data of
any other length is refused.

**`0`, create_poll: `poll_id` (8 B) ‖ `root` (32 B) ‖ `options` (1 B)**. Create poll `poll_id`
over the list rooted at `root`, with `options` options (2 to 8), each tally at zero. Accounts:
`authority` (writable signer, pays), `poll` (writable, PDA, not yet created), the system program.

**`1`, vote: `proof` (256 B) ‖ `nullifier` (32 B) ‖ `choice` (1 B)**. Verify a vote for `choice`
by some voter on the poll's list, create the nullifier's receipt, and add one to `tally[choice]`.
Accounts: `payer` (writable signer, any wallet: a relayer), `poll` (writable), `receipt` (writable,
PDA, not yet created), the system program. Public inputs: `Ballot` (`root, poll, nullifier,
{options, choice}`).

**`2`, close_poll: nothing**. Close the poll: no vote counts after this. Accounts: `authority`
(signer, the poll's), `poll` (writable).

### PDAs & state

| PDA | Seeds | Holds |
|---|---|---|
| `poll` | `["poll", poll_id as u64 LE]` | the poll, 144 B, below |
| `receipt` | `["voted", poll, nullifier]` | nothing: that it exists is the vote |

| Offset | Size | Field |
|--:|--:|---|
| 0 | 1 | tag, `1` |
| 1 | 1 | bump |
| 2 | 1 | `options` |
| 3 | 1 | open: `1`, or `0` once closed |
| 4 | 4 | reserved, zero |
| 8 | 8 | `poll_id: u64` |
| 16 | 32 | `authority` |
| 48 | 32 | `root`, a little-endian field element |
| 80 | 64 | `tally: [u64; 8]` |

### Errors

The program's own errors are `custom program error` codes, and each logs its name. Account checks
fail with the runtime's standard errors.

| Code | Error | When |
|--:|---|---|
| 0 | `InvalidProof` | the proof doesn't verify against the ballot the program assembled |
| 1 | `AlreadyVoted` | the nullifier's receipt exists: this voter has voted in this poll |
| 2 | `PollClosed` | the authority has closed the poll |
| | `InvalidArgument` | `create_poll` with fewer than 2 options or more than 8 |
| | `InvalidSeeds` | an account isn't the PDA it should be |
| | `IncorrectAuthority` | `close_poll` signed by someone other than the poll's authority |
| | `MissingRequiredSignature` | the authority or the payer didn't sign |

Previous ← [04 · Shielded pool](../04-shielded-pool/)

Next → [06 · Shielded transfer](../06-shielded-transfer/): Merkle trees and range proofs together
