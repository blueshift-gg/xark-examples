# 5 - Your ZK journey

You now have the mental model: proofs, circuits, Groth16, and trusted setup. Here is the concrete
path from this repository to designing a private Solana application.

## The path

**Step 1 - Run example 01.**
Build a Rust range circuit, export its verifier, prove with its generated TypeScript client, and
verify onchain in LiteSVM. The goal is to understand the build -> setup -> export -> client ->
onchain loop, and the range proof itself: a prime field has no order, so a circuit compares numbers
by decomposing them into bits.

**Step 2 - Learn xark's Rust circuit subset.**
Start with `Private<Field>`, `Public<Field>`, `#[input]` structs, sized comparisons, arrays, fixed loops,
and `require*`. Use `xark check` continuously: an unsupported Rust construct should be redesigned,
not worked around.

**Step 3 - Put range proofs to work, with commitments.**
[Example 02, zKYC](../../02-zkyc/) turns a bare range proof into a claim about a specific
committed identity, checked against a policy the program picks: range proofs for the age and the net
worth, an allow-list that hides which country, and a salted commitment that reveals nothing. Most
useful ZK applications need this kind of binding.

**Step 4 - Merkle trees, then stateful privacy.**
[Example 03](../../03-token-claim/) brings Merkle trees into the circuit: a leaf of address and
amount, the path a private input, membership in a list of a million in the same 256-byte proof.
[Example 04](../../04-shielded-pool/) puts 02's salted commitments in that tree and adds nullifiers,
recent roots, and a tree the program grows onchain. [Example 05](../../05-shielded-transfer/) brings
range proofs back: private values of any amount, each range-checked so the balance can't wrap the
field, with keys, SPL tokens, and encrypted note delivery.

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
- Vitalik Buterin's *Zk-SNARKs: Under the Hood* or Maksym Petkus's *Why and How zk-SNARK Works* for
  the underlying intuition.

The discipline is straightforward: keep the public contract small, constrain every dangerous
integer, pin the toolchain, use a real ceremony, test against the actual verifier, and document
the operational model around proof generation.

<- [Learn ZK](../README.md) | [Examples](../../README.md)
