# 03 · Token claim: a Merkle airdrop

A distributor publishes one 32-byte root for a list of `(address, amount)` pairs and funds a vault of
an SPL token. Each address on the list proves it's there, for its amount, and the program pays it,
once. [02](../02-zkyc/) proved something about one committed value; this proves
membership in a list of a million, with the same constant-size proof. Its one new idea is **a Merkle
path as a private input**.

> **Educational reference, not an audited product.** It uses a single-party setup key; for anything
> that holds value, use keys from a multi-party ceremony (`xark ceremony`).

## The list is a tree

Each claim is a leaf, and the list is the leaves in order, padded with an empty leaf to 2^20:

```
leaf = Poseidon2.hash([claimant.hi, claimant.lo, amount])
node = Poseidon2.hash2(left, right)
```

A Solana address is wider than the BN254 field, so it enters as xark's `Hash`, two 16-byte halves,
as 02's holder does. Leaves and nodes are both Poseidon2 (`xark-poseidon2`), but two different
functions of it, so they're separated without a tag. `hash` is a sponge whose capacity lane starts
at the input's length, 3, and a three-field leaf takes two permutations; `hash2` is one permutation
of `[left, right, 0]`, capacity 0. And the depth is fixed: every path has 20 levels, so a node can't
be passed off as a claim at a shallower one. The empty leaf is a constant, ASCII `xarkzero`, that no
claim hashes to, so the padding can't be claimed.

## The circuit

```rust
#[input]
pub struct Claim {
    pub claimant: Hash,
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
        let bits = Field::from(self.index).to_bits::<DEPTH>();
        let mut node = leaf;
        let mut i = 0;
        while i < DEPTH {
            // The bit orders the pair: 0, this node is the left child; 1, the right.
            let (left, right) = cond_swap(bits[i], node, self.siblings[i]);
            node = hash2(left, right);
            i += 1;
        }
        node
    }
}

#[circuit]
pub fn token_claim(path: Private<Path>, root: Public<Field>, claim: Public<Claim>) {
    require_eq(path.root(claim.leaf()), root);
}
```

- **Private:** the `Path`, its 20 siblings and the leaf's index. `to_bits::<20>()` decomposes the
  index into the bits that steer each level, and binds it below 2^20; `cond_swap` (`xark-merkle`'s,
  its rows proven in Lean) constrains each bit to 0 or 1 and orders the pair.
- **Public:** `root`, then `claim`: `claimant` (hi, lo) and `amount`. Four field elements.
- **No `bind`.** 02 needed `bind(holder)` because nothing else used the holder. Here the leaf hashes
  `claimant` and `amount`, and `amount` is a `u64`, range-checked on entry, so every public input is
  already in the constraints. A proof holds only for the claimant and amount it was made for.

## The list's builder is the circuit's Rust

The distributor and the claimant both need the circuit's hashing: the distributor for the root it
publishes, the claimant for the path it proves. Neither reimplements it. `Path` carries host-only
functions (`#[cfg(not(xark))]`), and `#[input] impl` puts them in the generated TypeScript client
beside `Claim.leaf`:

| Function | What it is |
|---|---|
| `Path.first()` | the first leaf's path, in the empty list |
| `tip.next(leaf)` | the next free leaf's path, once `leaf` is added at `tip` |
| `path.update(later, leaf)` | `path`, once `leaf` is added at a later leaf's path |
| `tip.listRoot()` | for the next free leaf's path, the root of the list so far |
| `path.root(leaf)` | the root with `leaf` at `path`: what the circuit checks |

Built from the [test](./client/token-claim.test.ts), the whole list:

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
quadratic in the list: fine for the test's three, too slow for a million. A real distributor hashes
the tree once, level by level, in Rust with the same `xark_poseidon2::hash2` (as the circuit's tests
do to check the builder), and hands each claimant its path, which the claimant checks with
`path.root(Claim.leaf(claim))` before proving. The circuit's tests check every root and path the
builder makes against that tree hashed from scratch.

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

The [program](./program/src/lib.rs) is Pinocchio, as 01's and 02's are, with `pinocchio-token` for
the SPL calls and plain PDAs for its state. `initialize` stores the root and the mint, and creates
the vault, a token account the distributor PDA owns, funded with `total` from the authority's
account. `claim` assembles the public inputs itself and makes one call to the verifier crate `xark
export` generated:

```rust
fn public_inputs(root: &[u8; 32], claimant: &[u8; 32], amount: u64) -> [[u8; 32]; 4] {
    let [hi, lo] = halves(claimant);
    [*root, hi, lo, field(amount)]
}

let public_inputs = public_inputs(&root, claimant.address().as_array(), amount);
if !verify(&Proof::from_le_bytes(proof), &public_inputs) {
    return Err(ProgramError::InvalidInstructionData);
}
```

then creates the receipt and transfers `amount` from the vault to the token account the claimant
names, signed by the distributor PDA. The root comes from the distributor's account, the claimant
from the signer, the amount from the instruction: a proof made for another list, another wallet or
another amount fails. The program's own test checks the demo proof (`examples/demo.rs`) against
what it assembles, and that another claimant, amount or root fails.

Anyone can send lamports to an address, and the system program's `CreateAccount` refuses one that
holds any: a griefer could block a claim by funding its receipt's address first. So the program
tops such an account up and assigns it instead, and the test does exactly that to Alice's.

## Run it

```bash
npm run build           # circuit → key (single-party) → demo proof → verifier crate → client → .so
npm run test:circuit    # the builder against the whole tree, who claims, what's refused; the program's
npm test                # prove with the client, claim onchain (LiteSVM)
```

`npm run build` makes a single-party key from this machine's randomness (see
[../RUNBOOK.md](../RUNBOOK.md)), and proves `examples/demo.rs` as the verifier crate's self-test.

The [test](./client/token-claim.test.ts) is the distributor and the claimant: a list of three, a
vault funded with their total, and Bob's claim of 1,000. Before it lands, each of these is refused
by the program: Bob's proof for another amount, the same proof signed by Mallory, and the proof
against a second distributor whose list also has Bob for 1,000 (another root). Bob's claim pays
him; claiming again finds his receipt; Alice claims hers, though someone funded her receipt's
address to block it, and Carol's stays in the vault. An address not on the list has no proof at
all: the client throws `CheckFailed`, naming `path.root(claim.leaf())`.

## Numbers

| Circuit | Constraints | Public inputs | Prove, 1 thread | Prove, 8 workers | Onchain CU |
|---|--:|--:|--:|--:|--:|
| `token_claim` | 5,437 | 4 | 989 ms | 195 ms | 97,846–103,846 |

Constraints from `xark build` (`xark profile` attributes them: 5,341 to the path and leaf, 96 to the
`u32` index and `u64` amount checks). Proving in node 22 through the generated client, median of
five, on an Apple M3 Max; "8 workers" is `@blueshift-gg/xark`'s worker pool. Compute units are the
whole `claim` in LiteSVM: the verify, the receipt it creates, and the SPL transfer. They vary in
steps of 1,500, one per extra try of `find_program_address` for the receipt's bump, which depends on
the claimant's address. A proof is always 256 bytes.

## Anatomy

```
circuit/    Claim (the leaf), Path (the tree, for the circuit and the list's builder), the
            circuit, and the demo proof that's the verifier crate's self-test
client/     the generated client, and the distributor and claimant test
program/    Pinocchio + SPL: store the root, hold the vault, verify, write the receipt, pay
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

An address enters the circuit as a `Hash`, `[hi, lo]`: each 16-byte half read as a big-endian
integer, as a little-endian field element.

### Errors

| Error | When |
|---|---|
| `InvalidInstructionData` | the data is malformed, or the proof doesn't verify |
| `AccountAlreadyInitialized` | the claimant has claimed: the receipt exists |
| `InvalidSeeds` | an account isn't the PDA it should be |
| `InvalidAccountData` | the distributor isn't one `initialize` made |
| `MissingRequiredSignature` | the authority or the claimant didn't sign |

Next → [04 · Shielded pool](../04-shielded-pool/)
