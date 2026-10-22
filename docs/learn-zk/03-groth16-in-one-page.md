# 3 · Groth16 in one page

There are many proof systems (Groth16, PLONK, Honk, STARKs, …). These examples use **Groth16**, and
here's the minimal picture of why and how — enough to use it well, not a cryptography
course.

## Why Groth16 for Solana

- **Tiny, fixed-size proofs: ~256 bytes**, no matter how big the circuit. Perfect for putting in a
  transaction.
- **Cheap verification** — a fixed pairing check, plus one scalar multiplication per public input.
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
prove(proving_key, public_inputs, witness)  ->  proof         (off-chain: `xark prove`, or a client)
verify(verifying_key, public_inputs, proof) ->  true / false  (on-chain, in your program)
```

The verifier folds the **public inputs** into its check, so a proof is bound to the exact public
inputs it was made with: change the recipient in a withdrawal and the proof stops verifying.
([Example 03](../../03-shielded-pool/) relies on this to stop front-running.) One subtlety: a public
input is folded in *through the constraints that use it*. One that no constraint touches drops out
of the check and verifies with any value. `xark build` refuses such a circuit; a value the circuit
only needs bound, like that recipient, gets one cheap constraint (`pool::bind`, after Tornado's
`recipientSquare`).

## What xark hands you

- `xark setup` → proving key + verifying key.
- `xark client` → a typed TypeScript client: prove (and verify) in the browser or node.
- `xark export` → a Rust verifier crate with the verifying key baked in.

A proof leaves the client as 256 bytes (`proof.toBytes()`). On-chain, your program assembles the
public inputs itself, 32-byte little-endian field elements, and makes one call:
`verify(&Proof::from_le_bytes(&proof), &public_inputs)`. The `alt_bn128` syscalls do the heavy
lifting.

## One number to remember

**256 bytes, verified in about 74k compute units, plus a few thousand per public input.**
Measured in these examples (whole instructions, LiteSVM): 01's bare verify, with no public inputs,
is 73,912 CU; 03's deposit, with four, is 101,041; 04's whole transact, with 21 public inputs, two
new accounts and SPL transfers, is 190,000 to 204,000. A ZK proof compresses an arbitrarily large private
computation into that. That compression is the whole reason this is exciting for a blockchain.

Next: [what a trusted setup is, and why you should care →](./04-trusted-setup-intuition.md)
