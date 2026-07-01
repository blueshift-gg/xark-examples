# 5 · Your ZK journey

You now have the mental model: proofs, circuits, Groth16, trusted setup. Here's the concrete path
from here to shipping a private app on Solana — and a *short, curated* set of resources (no link
dumps; every one earns its place).

## The path

**Step 1 — Run example 01 (today, ~20 min).**
[`01-over-9000`](../../01-over-9000/). Prove a range, verify it on devnet. Goal: get the toolchain
working and *feel* the prove → export → verify loop end to end.

**Step 2 — Learn just enough Noir (a day).**
You only need the basics: `Field` vs sized ints, `assert`, arrays, functions, `pub` inputs. Read
the [Noir language docs](https://noir-lang.org/docs) and rewrite example 01 from scratch without
looking.

**Step 3 — Build with commitments (example 02).**
[`02-age-verification`](../../02-age-verification/). This is the leap from "toy proof" to "useful
claim" — binding a hidden value to a public commitment. Most real ZK apps are variations on this.

**Step 4 — Build a real primitive (example 03).**
[`03-shielded-pool`](../../03-shielded-pool/). Merkle trees, nullifiers, two circuits, on-chain
state. When you understand why the chain never hashes, you understand ZK system *design*, not just
circuits.

**Step 5 — Design your own.**
Pick a problem where something should be *hidden but provable*: private votes, sealed-bid auctions,
proof-of-solvency, private airdrops, compliance-without-doxxing. Sketch the public/private split
first (that's 80% of the design), then the circuit, then the program.

## What to actually understand deeply (vs. skim)

- **Understand deeply:** the public/private input split; what a commitment buys you; nullifiers and
  double-spend prevention; that public inputs are bound to the proof; trusted-setup safety.
- **Skim / trust the tools:** the pairing math, R1CS internals, the ceremony's cryptographic
  transcript. xark handles these; you don't need to reimplement them to build well.

## Curated resources

**Noir**
- [Noir docs](https://noir-lang.org/docs) — the language reference. Start here.
- [Awesome Noir](https://github.com/noir-lang/awesome-noir) — vetted circuits and libraries to read.

**ZK intuition (pick one, not all)**
- Vitalik Buterin, *"Zk-SNARKs: under the hood"* — the best from-first-principles explainer if you
  want the math to click.
- *"Why and How zk-SNARK Works"* (Maksym Petkus) — a longer, patient derivation if you want depth.

**Groth16 / setup**
- The [snarkjs](https://github.com/iden3/snarkjs) README — walks the setup → prove → verify flow
  concretely (and xark is snarkjs-compatible, so it transfers directly).

**xark itself**
- The [xark repo](https://github.com/blueshift-gg/xark) — `docs/architecture.md` for how it fits
  together, `docs/trusted-setup.md` for ceremonies, `docs/security.md` for the soundness story.

## A note on taste

The ZK space is loud with jargon and hype. Ignore most of it. The teams that ship pick one proof
system, understand their trust assumptions, keep circuits small, and test relentlessly against a
real verifier. That's the whole discipline. You already have the pieces — go build something that
should have been private all along.

← back to [Learn ZK](../README.md) · [the examples](../../README.md)
