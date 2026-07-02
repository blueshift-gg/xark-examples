# 03 · Shielded pool — unlinkable transfers (Tornado++)

Deposit a fixed amount, then withdraw it to a fresh address such that nobody can link the withdrawal
to the deposit. On-chain an observer sees a deposit and, later, an unrelated-looking withdrawal. This
is the first real privacy primitive in the ladder: it combines proofs ([01](../01-over-9000/)) and
commitments ([02](../02-age-verification/)) with Merkle membership and nullifiers.

> **Reference implementation. Unaudited. Educational.** Mixers carry real legal and regulatory weight
> depending on where you are. Never use `--insecure-dev-mode` keys for anything that holds value.

## How it works

Every deposit puts a **commitment** — `Poseidon2(nullifier, secret)` — as a leaf in a Merkle tree.
To withdraw, you prove in zero knowledge that:

1. **membership** — your commitment is *some* leaf in the tree, without revealing which; and
2. **no double-withdraw** — you reveal `nullifier_hash = Poseidon2(nullifier)`, a value only the
   note's owner can produce, which the pool records so that note can never be spent twice.

Unlinkability comes from (1): you prove you own *a* leaf, not *which* leaf, so the withdrawal can't
be tied to any particular deposit. Your anonymity set is every deposit in the tree — which is exactly
why a mixer is only as private as it is used.

### The design choice that makes it clean: the chain never hashes

A shielded pool is a Merkle tree, and the classic trap is that the Poseidon in your circuit and the
Poseidon on-chain must be **byte-for-byte identical** or the roots never match. Instead of
reimplementing Barretenberg's Poseidon2 inside a Solana program, keep *all* hashing in Noir:

- **`deposit` circuit** proves "`new_root` is the correct result of appending `leaf` at `index` to
  the tree rooted at `old_root`." It binds the depositor-supplied frontier (the left siblings on the
  path to the next slot) to `old_root` — only the true frontier reproduces it — then computes the new
  root. Public inputs: `old_root, new_root, leaf, index`.
- **`withdraw` circuit** proves membership and derives the nullifier hash. Public inputs:
  `root, nullifier_hash, recipient_hi, recipient_lo, relayer_hi, relayer_lo, fee`.

The program then stores only a ring of recent roots, `next_index`, and one marker account per spent
nullifier. It verifies proofs and moves lamports — zero Poseidon on-chain, tiny CU, and circuit/chain
agreement holds *by construction* instead of by careful matching.

```
deposit:   nargo builds new_root ─▶ deposit proof ─▶ program checks proof, stores new_root
withdraw:  nargo builds path     ─▶ withdraw proof ─▶ program checks proof + nullifier, pays out
```

**Front-run protection.** Recipient and relayer are each split into two 128-bit halves (a Solana
pubkey is 256-bit, wider than the BN254 field) and passed as public inputs. Groth16 binds public
inputs to the proof, so a relayer can't rewrite the destination of your withdrawal.

**Multi-denomination (the "++").** Each denomination is its own pool — `initialize(denomination)`
creates a PDA seeded by the amount, exactly like Tornado's separate 0.1 / 1 / 10 pools. One deployed
program serves every size, and nullifiers are namespaced per denomination so pools stay independent.
Amounts are hidden *within* a pool because every note in it has identical value; arbitrary hidden
amounts need value-carrying notes, which is [04](../04-shielded-transfer/).

## Anatomy

```
circuits/deposit/    proves a correct tree append   (public: old_root, new_root, leaf, index)
circuits/withdraw/   proves membership + nullifier   (public: root, nullifier_hash, recipient×2, relayer×2, fee)
program/             Anchor: verify both proofs, hold + pay lamports, track roots & nullifiers
```

## Run it

```bash
just prove-deposit    # prove + export the deposit circuit
just prove-withdraw   # prove + export the withdraw circuit
just build-program
cd ../e2e && cargo test shielded_pool_full_flow   # init → deposit → withdraw → double-spend rejected
```

Witness values (paths, roots, nullifiers) are generated with nargo — the same Poseidon as the
circuit, so they always agree; see the worked index-0 example in
[`../e2e/tests/pool.rs`](../e2e/tests/pool.rs), which doubles as the reference client. To deploy,
`anchor deploy` and drive `initialize → deposit → withdraw`; the exact instruction encodings and
account metas are in that test.

## What it gives you — and doesn't

**Gives you:** unlinkability between deposit and withdrawal across the anonymity set; double-spend
protection via one-time nullifiers; front-run protection from binding recipient/relayer/fee into the
proof.

**Doesn't:** hide amounts across denominations (fixed size per pool); resist timing/amount
correlation if the set is tiny or you withdraw instantly; offer any compliance layer (viewing keys,
association sets are deliberately out of scope); or carry an audit. `ZERO` (the empty-leaf sentinel)
is `0` here for clarity — a production pool uses a nothing-up-my-sleeve nonzero value so no real
commitment can collide with an empty slot.

## Reference

### Instructions

**`initialize(denomination: u64)`** — create the pool for a denomination (the empty-tree root is
fixed on-chain). Accounts: `pool` (init, PDA), `authority` (signer, payer), `system_program`.

**`deposit(commitment: [u8;32], new_root: [u8;32], proof: Vec<u8>)`** — deposit the denomination and
append `commitment`; `proof` attests that `new_root` extends the tree by one leaf. Accounts: `pool`
(mut), `depositor` (signer), `system_program`. Public inputs: `old_root, new_root, leaf, index`.

**`withdraw(proof: Vec<u8>, root: [u8;32], nullifier_hash: [u8;32], fee: u64)`** — pay
`denomination − fee` to `recipient` and `fee` to the relayer; the `nullifier` PDA is created here, so
a repeat with the same nullifier fails. Accounts: `pool` (mut), `nullifier` (init, PDA), `recipient`
(unchecked, mut), `relayer` (signer, payer), `system_program`. Public inputs:
`root, nullifier_hash, recipient_hi, recipient_lo, relayer_hi, relayer_lo, fee`.

### PDAs & state

| PDA | Seeds |
|---|---|
| `pool` | `["pool", denomination as u64 LE]` |
| `nullifier` | `["nullifier", denomination as u64 LE, nullifier_hash]` |

`Pool` state: `denomination: u64`, `next_index: u32`, `current_root_index: u32`,
`roots: [[u8;32]; 16]` (the recent-root ring), `bump: u8`. A pubkey enters the circuit as two 128-bit
halves — `lo = bytes[0..16]`, `hi = bytes[16..32]`, each little-endian in a field.

### Errors

| Error | Message |
|---|---|
| `BadProofLen` | proof must be 256 bytes |
| `InvalidProof` | invalid proof |
| `UnknownRoot` | unknown or stale merkle root |
| `FeeTooHigh` | fee exceeds denomination |
| `TreeFull` | tree is full |

Next → [04 · Shielded transfer](../04-shielded-transfer/)
