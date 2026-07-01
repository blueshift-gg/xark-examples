# 3 · Groth16 in one page

There are many proof systems (Groth16, PLONK, Honk, STARKs, …). These examples use **Groth16**, and
here's the honest, minimal picture of why and how — enough to use it well, not a cryptography
course.

## Why Groth16 for Solana

- **Tiny, fixed-size proofs: ~256 bytes**, no matter how big the circuit. Perfect for putting in a
  transaction.
- **Cheap, constant-time verification** — a handful of elliptic-curve pairings.
- **Solana verifies it natively.** The runtime exposes `alt_bn128` syscalls (addition,
  multiplication, pairing) over the **BN254** curve — exactly what Groth16 needs. That's why xark
  targets Groth16/BN254 specifically: the verifier is small and syscall-accelerated.

The tradeoff you're accepting: Groth16 needs a **per-circuit trusted setup** (next page). Newer
systems avoid that but have bigger proofs or heavier verification. For "verify a fixed circuit
cheaply on Solana," Groth16 is the pragmatic pick.

## The mental model

After setup you have two keys:

- a **proving key** (big, used off-chain to make proofs), and
- a **verifying key** (small, embedded in your on-chain program by `xark export`).

Then:

```
prove(proving_key, public_inputs, witness)  ->  proof         (off-chain, in `xark prove`)
verify(verifying_key, public_inputs, proof) ->  true / false  (on-chain, in your program)
```

The verifier folds the **public inputs** into its check. This is a subtle, important point: a proof
is bound to the exact public inputs it was made with. Change the recipient in a withdrawal and the
proof stops verifying — you don't need extra constraints to "lock" a public input; being public
already binds it. ([Example 03](../../03-shielded-pool/) relies on this to stop front-running.)

## What xark hands you

- `xark setup` → proving key + verifying key (+ snarkjs-compatible JSON).
- `xark prove` → `proof.bin` + `public_inputs.bin` (+ snarkjs JSON).
- `xark export` → a Rust verifier crate with the verifying key baked in, plus
  `instruction_data.bin` = `proof (256 B) || public_inputs (N × 32 B, little-endian)` — the exact
  bytes your program receives.

On-chain, your program does one thing: `verify_instruction_data(data)`. The `alt_bn128` syscalls do
the heavy lifting.

## One number to remember

**~256 bytes, verified in ~a few hundred k compute units.** A ZK proof compresses an arbitrarily
large private computation into that. That compression is the whole reason this is exciting for a
blockchain.

Next: [what a trusted setup is, and why you should care →](./04-trusted-setup-intuition.md)
