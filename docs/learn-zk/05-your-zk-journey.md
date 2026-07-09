# 5 - Your ZK journey

You now have the mental model: proofs, circuits, Groth16, and trusted setup. Here is the concrete
path from this repository to designing a private Solana application.

## The path

**Step 1 - Run example 01.**
Build a Rust range circuit, create a proof, export its verifier, and execute it in LiteSVM. The goal
is to understand the build -> setup -> prove -> export -> on-chain loop.

**Step 2 - Learn xark's Rust circuit subset.**
Start with `Private<Field>`, `Public<Field>`, sized comparisons, arrays, fixed loops, and assertions.
Use `xark check` continuously: an unsupported Rust construct should be redesigned, not worked around.

**Step 3 - Build with commitments.**
[Example 02](../../02-age-verification/) turns a bare range proof into a claim about a specific
committed value. Most useful ZK applications need this kind of binding.

**Step 4 - Study stateful privacy.**
[Example 03](../../03-shielded-pool/) adds Merkle membership, nullifiers, recent roots, and real
on-chain state. [Example 04](../../04-shielded-transfer/) adds private values, keys, SPL tokens, and
encrypted note delivery.

**Step 5 - Design the application contract first.**
Write down what is public, what is private, who vouches for each public value, how proofs become
stale, and which state transitions the program enforces. Only then write the circuit.

## What to understand deeply

- The public/private input split and exact public-input ordering.
- What commitments bind and who is trusted to publish them.
- Nullifier derivation and double-spend prevention.
- Range bounds and finite-field wraparound.
- Trusted-setup provenance and upgrade-authority policy.
- Wallet synchronization, note delivery, and stale-proof recovery.

You can usually treat pairing internals as library machinery, but not the application contract
around them.

## Resources

- The [xark repository](https://github.com/blueshift-gg/xark): architecture, integer semantics,
  trusted setup, and security documentation.
- The [Rust reference](https://doc.rust-lang.org/reference/) for the source language; remember that
  xark deliberately accepts only a sound circuit subset.
- The [snarkjs repository](https://github.com/iden3/snarkjs) for an independent Groth16 artifact
  consumer. xark emits snarkjs-compatible proof and verification-key JSON.
- Vitalik Buterin's *Zk-SNARKs: Under the Hood* or Maksym Petkus's *Why and How zk-SNARK Works* for
  the underlying intuition.

The discipline is straightforward: keep the public contract small, constrain every dangerous
integer, pin the toolchain, use a real ceremony, test against the actual verifier, and document
the operational model around proof generation.

<- [Learn ZK](../README.md) | [Examples](../../README.md)
