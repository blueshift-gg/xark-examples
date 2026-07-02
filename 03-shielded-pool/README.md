# 03 · Shielded pool — unlinkable transfers

> Goal: deposit a fixed amount of SOL, then withdraw it to a fresh address such
> that **nobody can link the withdrawal to the deposit** — the on-chain graph
> shows a deposit and, later, an unrelated-looking withdrawal.

This is the real thing: Merkle membership, nullifiers, and two cooperating
circuits. It combines everything from [01](../01-over-9000/) (proofs) and
[02](../02-age-verification/) (commitments) into a working privacy primitive.

> ⚠️ **Reference implementation. Unaudited. Educational.** Mixers carry real
> legal and regulatory weight depending on where you are. Understand that
> before building on this. Never use `--insecure-dev-mode` keys for anything real.

## How it works

Every deposit puts a **commitment** — `Poseidon2(nullifier, secret)` — into a Merkle tree. To
withdraw you prove, in zero knowledge, that:

1. your commitment is a leaf in the tree (**membership**), and
2. you reveal `nullifier_hash = Poseidon2(nullifier)` so the pool can mark that note spent
   (**no double-withdraw**),

without revealing *which* leaf is yours. The anonymity set is every deposit in the tree.

### The design choice that makes it clean: the chain never hashes

A shielded pool is a Merkle tree, and the classic pain is that **the Poseidon in your circuit and
the Poseidon on-chain must be byte-for-byte identical** or roots never match. Instead of
re-implementing Barretenberg's Poseidon2 inside a Solana program, we keep *all* hashing in Noir:

- **`deposit` circuit** proves "`new_root` is the correct result of appending `leaf` at `index` to
  the tree currently rooted at `old_root`." It binds the depositor-supplied frontier to `old_root`
  (only the true frontier reproduces it), then computes the new root.
- **`withdraw` circuit** proves membership + derives the nullifier hash.

The **program stores only** `{ recent-roots ring, next_index, one marker account per spent
nullifier }`. It verifies proofs and moves lamports — zero Poseidon, tiny CU, and consistency
between circuit and chain is true *by construction*.

```
deposit:   off-chain (nargo) builds new_root ─▶ deposit proof ─▶ program checks proof, stores new_root
withdraw:  off-chain (nargo) builds path     ─▶ withdraw proof ─▶ program checks proof + nullifier, pays out
```

## Anatomy

```
circuits/deposit/    proves a correct tree append           (public: old_root,new_root,leaf,index)
circuits/withdraw/   proves membership + nullifier           (public: root,nullifier_hash,recipient*,relayer*,fee)
program/             Anchor: verify both proofs, hold + pay lamports, track roots & nullifiers
```

The full flow is proven end-to-end in [`../e2e/tests/pool.rs`](../e2e/tests/pool.rs):
`cd ../e2e && cargo test shielded_pool_full_flow` runs init → deposit → withdraw →
double-spend-rejected in LiteSVM. (The reference client is that Rust test — the earlier TypeScript
client was removed.)

Recipient and relayer are each split into two 128-bit halves (a Solana pubkey is 256-bit, larger
than the BN254 field). Because they're public inputs, Groth16 binds them to the proof — so a
relayer can't redirect your withdrawal.

## Run it (devnet)

Prerequisites and the one-time `xark-verifier` path setup are in [../RUNBOOK.md](../RUNBOOK.md).

```bash
# 1. Fill each circuit's Prover.toml with witness values. These are generated
#    with nargo (same Poseidon as the circuit, so they always agree) — see the
#    worked index-0 example in ../e2e/tests/pool.rs.

# 2. Prove + export both circuits, then build the program
just prove-deposit         # generates the deposit verifier crate
just prove-withdraw        # generates the withdraw verifier crate
just build-program
cd program && anchor deploy

# 3. Drive it: initialize(denomination) → deposit(commitment, new_root,
#    proof) → withdraw(proof, root, nullifier_hash, fee). The exact instruction
#    encodings + account metas are in ../e2e/tests/pool.rs.
```

Want to see it actually run first? `cd ../e2e && cargo test shielded_pool_full_flow` executes the
entire deposit → withdraw flow (and a rejected double-spend) in an in-process Solana VM.

## What it does and doesn't give you

✅ On-chain **unlinkability** between deposit and withdrawal, across the anonymity set.
✅ **Double-spend protection** via one-time nullifiers.
✅ **Front-run protection** — recipient/relayer/fee are bound into the proof.

❌ Fixed denomination only (arbitrary amounts need a UTXO/note design — a natural v2).
❌ Timing/amount correlation still leaks if the anonymity set is tiny or you withdraw instantly.
❌ No compliance features (viewing keys, association sets) — deliberately out of scope here.
❌ Not audited. The circuits and program are written to teach the mechanism clearly, not to be safe
   to hold value.

## Notes on correctness (read before trusting a proof)

- Confirm the **public-input order** each circuit exposes with `xark inspect`, and that it matches
  the order `program/src/lib.rs` builds (`deposit`: old_root,new_root,leaf,index — `withdraw`:
  root,nullifier_hash,recipient_hi,recipient_lo,relayer_hi,relayer_lo,fee).
- The circuits use a fixed-arity Poseidon2 compression over `std::hash::poseidon2_permutation`
  (beta.22 only exposes the permutation). Any off-chain input generator must reproduce it exactly —
  the e2e test uses nargo, which is consistent by construction.
- `ZERO` (empty leaf) is `0` here for clarity; a production pool uses a nothing-up-my-sleeve nonzero
  value so no real commitment can collide with an empty slot.

You've now built a real private application on Solana with Noir + xark. Back to the
[gallery](../README.md).
