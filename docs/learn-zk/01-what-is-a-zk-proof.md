# 1 · What is a ZK proof?

One idea, and it's this simple:

> **A zero-knowledge proof lets you convince someone that a statement is true, while revealing
> nothing beyond the fact that it's true.**

That's it. Everything else is machinery for making that practical.

## A concrete example

"I am over 18." Normally you prove this by showing an ID — which also reveals your name, exact
birthday, address, and document number. That's *way* more than the claim.

A ZK proof lets you hand over a small blob of bytes that convinces a verifier "this person is 18+"
and **nothing else**. No birthday. No name. The verifier checks the blob and gets a yes/no.

You'll build exactly this in [example 02](../../02-age-verification/).

## Three roles

- **Prover** — has a secret (the *witness*) and wants to prove a statement about it. Runs the
  expensive part (making the proof).
- **Verifier** — checks the proof. Cheap and fast. On our examples, this is a Solana program.
- **Statement** — the public claim being proven, e.g. "the committed birth year is ≤ 2008."

## Public vs private inputs — the whole game

Every circuit input is one of two kinds:

- **Private (witness):** known only to the prover. Your birth year, your password, your balance.
  Never appears in the proof.
- **Public:** known to everyone, part of the statement. A commitment, a Merkle root, a threshold.

Designing a ZK application is mostly deciding **what's private, what's public, and what must be
true relating them.** Get that split right and the rest follows.

## The three properties (what "proof" actually guarantees)

1. **Completeness** — if the statement is true and you follow the rules, the proof verifies.
2. **Soundness** — if the statement is false, you *cannot* produce a proof that verifies (except
   with negligible probability). This is what makes it trustworthy.
3. **Zero-knowledge** — the proof leaks nothing about the witness beyond the statement's truth.

## What it's good for onchain

- **Privacy:** hide amounts, identities, positions — while still proving they're valid.
- **Compression / verifiable compute:** prove a big computation ran correctly, verify it cheaply.
  (Groth16 proofs are ~256 bytes and verify in milliseconds, regardless of how big the computation
  was.)

On Solana specifically, the chain can verify these proofs natively via the `alt_bn128` syscalls —
which is the whole reason xark targets Groth16 over the BN254 curve.

Next: [how a circuit turns code into something provable →](./02-circuits-and-constraints.md)
