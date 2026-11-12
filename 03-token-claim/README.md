# 03 · Token claim: a Merkle airdrop

A distributor publishes one 32-byte root for a list of `(address, amount)` pairs and funds a vault of
an SPL token. Each address on the list proves it's there, for its amount, and the program pays it,
once. [02](../02-zkyc/) proved something about one committed value; this proves
membership in a list of a million, with the same constant-size proof. Its one new idea is **the
Merkle tree in a circuit**: each leaf is the Poseidon2 hash of a claimant's Solana address (as two
halves) and the amount they're owed, and the claimant proves their leaf is under the published root
with the path as a private input. That proof is what entitles them to claim the amount.

> **Educational reference, not an audited product.** It uses a single-party setup key; for anything
> that holds value, use keys from a multi-party ceremony (`xark ceremony`).

## The list is a tree

Each claim is a leaf, and the list is the leaves in order, padded with an empty leaf to 2^20. The
tree is `xark-merkle`'s Poseidon2 tree (`xark_merkle::poseidon2`):

```
leaf = Poseidon2.hash([claimant.hi, claimant.lo, amount])
node = Poseidon2.hash2(left, right)
```

A Solana address is wider than the BN254 field, so it enters as xark's `Address`, two 16-byte
halves, as 02's holder does. Leaves and nodes are both Poseidon2 (`xark-poseidon2`), but two different
functions of it, so they're separated without a tag. `hash` is a sponge whose capacity lane starts
at the input's length, 3, and a three-field leaf takes two permutations; `hash2` is one permutation
of `[left, right, 0]`, capacity 0. And the depth is fixed: every path has 20 levels, so a node can't
be passed off as a claim at a shallower one. The empty leaf is a constant, ASCII `xarkzero`, that no
claim hashes to, so the padding can't be claimed.

## The circuit

```rust
use xark_merkle::poseidon2::merkle_root;

#[input]
pub struct Claim {
    pub claimant: Address,
    pub amount: u64,
}

#[input]
impl Claim {
    pub fn leaf(&self) -> Field { /* Poseidon2(claimant.hi, claimant.lo, amount) */ }
}

/// A leaf's authentication path: its index in the list, and its sibling at each height.
#[input]
pub struct Path {
    pub index: u32,
    pub siblings: [Field; 20],
}

#[input]
impl Path {
    /// The root of the tree with `leaf` at this path.
    pub fn root(&self, leaf: Field) -> Field {
        merkle_root(leaf, self.siblings, Field::from(self.index).to_bits::<DEPTH>())
    }
}

#[circuit]
pub fn token_claim(path: Private<Path>, root: Public<Field>, claim: Public<Claim>) {
    require_eq(path.root(claim.leaf()), root);
}
```

- **Private:** the `Path`, its 20 siblings and the leaf's index. `to_bits::<20>()` decomposes the
  index into the bits that steer each level, and binds it below 2^20; `merkle_root` folds the path,
  each level a `cond_swap` (its rows proven in Lean), which constrains the bit to 0 or 1 and orders
  the pair, then a `hash2`.
- **Public:** `root`, then `claim`: `claimant` (hi, lo) and `amount`. Four field elements.
- **No `bind`.** 02 needed `bind(holder)` because nothing else used the holder. Here the leaf hashes
  `claimant` and `amount`, and `amount` is a `u64`, range-checked on entry, so every public input is
  already in the constraints. A proof holds only for the claimant and amount it was made for.

## The list's builder is the circuit's Rust

The distributor and the claimant both need the circuit's hashing: the distributor for the root it
publishes, the claimant for the path it proves. Neither reimplements it. In Rust, the crate's
`list()` is the empty list as a `xark_merkle::tree::Tree`, the host builder of the tree the circuit
folds: push each claim's leaf, then take its `root()` and each claim's `path(i)` (the demo proof is
built this way). A `Tree` isn't an `#[input]` type, so it doesn't reach the TypeScript client;
`Path` carries host-only functions (`#[cfg(not(xark))]`) that do, through `#[input] impl`, beside
`Claim.leaf`:

| Function | What it is |
|---|---|
| `Path.first()` | the first leaf's path, in the empty list |
| `tip.next(leaf)` | the next free leaf's path, once `leaf` is added at `tip` |
| `path.update(later, leaf)` | `path`, once `leaf` is added at a later leaf's path |
| `tip.listRoot()` | for the next free leaf's path, the root of the list so far |
| `path.root(leaf)` | the root with `leaf` at `path`: what the circuit checks |

Built from the [test](./tests/token-claim.test.ts), the whole list:

```ts
let tip = Path.first();                       // the next free leaf's path
let paths: PathValue[] = [];
for (const claim of claims) {
  const leaf = Claim.leaf(claim);
  paths = paths.map((p) => p.update(tip, leaf));
  paths.push(tip);
  tip = tip.next(leaf);
}
const root = tip.listRoot();                  // what the distributor publishes
prover.prove({ path: paths[i], root, claim: claims[i] });
```

A generated client function takes `#[input]` types, which are fixed-size, so the list itself stays a
TypeScript array and the loop calls the Rust once per step. It updates every path at every step,
quadratic in the list: fine for the test's three, too slow for a million. A real distributor builds
the tree once in Rust with `list()` (`Tree::extend` hashes each node once), and hands each claimant
its path, which the claimant checks with `path.root(Claim.leaf(claim))` before proving. The
circuit's tests check every root and path `Path`'s builder makes against the `Tree`'s.

## What hiding the path buys

A Merkle airdrop without zero knowledge sends the path in the transaction and hashes it in the
program: 20 siblings, 640 bytes, and 20 hashes, all of which grow with the tree, and the program's
hash must match the distributor's byte for byte. Here the path stays in the proof:

- **Constant size.** A claim is the proof (256 bytes) and the amount (8 bytes), whatever the tree's
  depth: a circuit for a deeper tree takes the same transaction.
- **The program never hashes.** It checks one proof against the root it stores; there's no
  Poseidon2 onchain to keep in step with the circuit's. That agreement holds by construction.
- **The rest of the list stays offchain.** No sibling, and no index, ever appears in a transaction. A sibling leaf is the Poseidon2 hash of an
  address and an amount, which anyone can test against a known address; a claim here gives nothing
  away about the other entries.

The cost moves to the claimant's device: 5,437 constraints, 243 per level of the tree (a Poseidon2
`hash2`, a `cond_swap` and an index bit) and 480 for the leaf's two permutations, about a second to
prove in node on one thread, 0.2 s on 8 workers (see Numbers). Onchain, the proof's verification is
most of a claim's compute: a plain SHA-256 path would cost the program fewer compute units and more
bytes.

## What it doesn't hide

**The claimant and the amount are public.** The program builds them into the public inputs, from
the signer and the instruction, so anyone reading the chain sees who claimed how much. That's the
point of this example: a proof of membership, not of anonymity. Hiding who claims needs the
claimant to stay off the transaction, which means a nullifier and a relayer: that's the
[shielded pool](../04-shielded-pool/).

**No nullifier, because the claimant is public.** A shielded pool's withdrawer is hidden, so the
program can't remember *who* withdrew; it remembers a nullifier, a value only the note's owner can
derive. Here the program knows who claims: the signer. So the claim's receipt is a PDA seeded by the
claimant's address, created in the claim, and a second claim by the same address finds it and fails.
It can't key on the proof's bytes: a Groth16 proof is re-randomizable, and anyone holding one can
make another of the same statement.

## The program

The [program](./programs/token-claim/src/) is Pinocchio, as 01's and 02's are, with `pinocchio-token` for the
SPL calls and plain PDAs for its state. `lib.rs` dispatches on the first byte; each instruction is a
file under `instructions/`, with its accounts, its instruction data and its logic in their own
structs, the accounts' checks in their `TryFrom`. The program's address is `ID` in `lib.rs`, which
the client test deploys it at; the PDAs derive from it and the owner checks are against it.
`initialize` stores the root, the mint and both
bumps in the distributor (`state.rs`), and creates the vault, a token account the distributor PDA
owns, funded with `total` from the authority's account. `claim` checks the vault against the stored
bump, fills in the circuit's public inputs itself, by name, and makes one call to the verifier crate
`xark build` generated (`clients/token-claim/rust`, whose `PublicInputs` packs the address into its
two halves as the circuit does):

```rust
use token_claim_verifier::{Proof, PublicInputs, verify};

let inputs = PublicInputs {
    root,
    claim: token_claim_verifier::Claim {
        claimant: *accounts.claimant.address().as_array(),
        amount: data.amount,
    },
};
if !verify(&Proof::from_le_bytes(data.proof), inputs) {
    return Err(TokenClaimError::InvalidProof.into());
}
```

then creates the receipt and transfers `amount` from the vault to the token account the claimant
names, signed by the distributor PDA. The root comes from the distributor's account, the claimant
from the signer, the amount from the instruction: a proof made for another list, another wallet or
another amount fails. The receipt is the one PDA the program finds rather than derives from a
stored bump: it must sit at the canonical bump, or a claimant could claim again at another. The
program's own test checks the demo proof (`circuits/token-claim/examples/demo.rs`) against
the inputs it fills in, and that another claimant, amount or root fails.

Anyone can send lamports to an address, and the system program's `CreateAccount` refuses one that
holds any: a griefer could block a claim by funding its receipt's address first. So the program
tops such an account up and assigns it instead, and the test does exactly that to Alice's.

## Run it

```bash
xark build     # circuit, its clients (Rust verifier crate, TypeScript), then the program
xark test      # the builder against the Tree, who claims, what's refused; the program's; then
               # prove with the client and claim onchain (LiteSVM)
```

`npm run check` is CI's `xark build --check`: the committed keys and clients are what the source
builds. Prerequisites: [../RUNBOOK.md](../RUNBOOK.md) (`xark doctor` checks them).

The key in `keys/token-claim/` is a single-party setup from one machine's randomness. The verifier
crate bundles a sample proof as its self-test: `circuits/token-claim/examples/demo.rs` proves it
(`cargo run --release --example demo` in the circuit's directory, after a `xark build`), and the next
`xark build` keeps it while it verifies.

The [test](./tests/token-claim.test.ts) is the distributor and the claimant: a list of three, a
vault funded with their total, and Bob's claim of 1,000. Before it lands, each of these is refused
by the program: Bob's proof for another amount, the same proof signed by Mallory, and the proof
against a second distributor whose list also has Bob for 1,000 (another root). Bob's claim pays
him; claiming again finds his receipt; Alice claims hers, though someone funded her receipt's
address to block it, and Carol's stays in the vault. An address not on the list has no proof at
all: the client throws `CheckFailed`, naming `path.root(claim.leaf())`.

## Numbers

| Circuit | Constraints | Public inputs | Prove, 1 thread | Prove, 8 workers | Onchain CU |
|---|--:|--:|--:|--:|--:|
| `token_claim` | 5,437 | 4 | 989 ms | 195 ms | 97,874–103,874 |

Constraints from `xark build` (`xark profile` attributes them: 5,341 to the path and leaf, 96 to the
`u32` index and `u64` amount checks). Proving in node 22 through the generated client, median of
five, on an Apple M3 Max; "8 workers" is `@blueshift-gg/xark`'s worker pool. Compute units are the
whole `claim` in LiteSVM: the verify, the receipt it creates, and the SPL transfer. They vary in
steps of 1,500, one per extra try of `find_program_address` for the receipt's bump, which depends on
the claimant's address. A proof is always 256 bytes.

## Layout

```
circuits/token-claim/   Claim (the leaf), Path (the tree, for the circuit and the list's builder),
                        list() (the Rust builder), the circuit, and examples/demo.rs, the
                        verifier crate's sample proof
programs/token-claim/   Pinocchio + SPL: store the root, hold the vault, verify, write the receipt,
                        pay; lib.rs (dispatch), instructions/{initialize,claim}.rs, state.rs (the
                        distributor), errors.rs (InvalidProof), helpers.rs (account creation, logging)
clients/token-claim/    generated by `xark build`: the Rust verifier crate and the TypeScript client
keys/token-claim/       the proving and verifying keys (committed: the verifier embeds the key)
tests/                  the distributor and claimant test (LiteSVM)
xark.toml               the xark release the project is made with
```

## Deployment boundaries

- **One entry per address.** The receipt is per address, so a second entry for the same address
  can never be claimed. The builder doesn't check this; the distributor's list should.
- **The vault funds the claims.** `initialize` takes `total` and doesn't check it covers the list;
  a claim past what the vault holds fails at the transfer. Anyone can top it up with a plain SPL
  transfer.
- **No clawback or expiry.** Unclaimed tokens stay in the vault: a real distributor would add an
  authority that can close it after a deadline.
- **Dev trusted setup.** A real deployment needs `xark ceremony`.

## Reference

### Instructions

Instruction data starts with one byte, which instruction; integers are little-endian.

**`0`, initialize: `root` (32 B) ‖ `total` (8 B)**. Create the distributor for the list rooted at
`root`, and its vault, funded with `total`. Accounts: `authority` (signer, writable: the payer),
`distributor` (writable, PDA), `vault` (writable, PDA), `mint`, `authority_token` (writable: the
source of `total`), the token program, the system program.

**`1`, claim: `proof` (256 B) ‖ `amount` (8 B)**. Verify that the signer is on the list for
`amount`, create the receipt, and pay `amount` from the vault. Accounts: `claimant` (signer,
writable: the payer), `distributor`, `vault` (writable), `receipt` (writable, PDA),
`claimant_token` (writable, of the distributor's mint: the destination), the token program, the
system program. Public inputs: `root`, `claim` (`claimant (hi, lo), amount`).

### PDAs & state

| PDA | Seeds | Holds |
|---|---|---|
| `distributor` | `["distributor", root]` | `root` (32 B) ‖ `mint` (32 B) ‖ its bump ‖ the vault's |
| `vault` | `["vault", distributor]` | an SPL token account, owner `distributor` |
| `receipt` | `["claimed", distributor, claimant]` | nothing: that it exists is the claim |

An address enters the circuit as an `Address`, `[hi, lo]`: each 16-byte half read as a big-endian
integer, as a little-endian field element.

### Errors

The program's one error of its own is `TokenClaimError::InvalidProof`, custom code 0; the rest are
built-in `ProgramError`s. The program logs its own error's name; the runtime names the rest.

| Error | When |
|---|---|
| `InvalidProof` (custom 0) | the proof doesn't verify for the root, the signer and the amount |
| `InvalidInstructionData` | the data is malformed, or the first byte is no instruction |
| `AccountAlreadyInitialized` | the claimant has claimed: the receipt exists |
| `InvalidSeeds` | an account isn't the PDA it should be |
| `InvalidAccountData` | the distributor isn't one `initialize` made |
| `MissingRequiredSignature` | the authority or the claimant didn't sign |
| `IncorrectProgramId` | the token or the system program isn't the one named |
| `NotEnoughAccountKeys` | fewer accounts than the instruction names |

Next → [04 · Shielded pool](../04-shielded-pool/): Merkle trees plus salted commitments
