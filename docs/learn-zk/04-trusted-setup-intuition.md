# 4 · Trusted setup, intuitively

Groth16's one catch: before you can prove a circuit, someone runs a **setup** that produces the
proving and verifying keys. The setup uses a secret random value — informally, **"toxic waste"** —
that **must be destroyed**. If someone keeps it, they can forge proofs that verify for false
statements. Understanding this is the difference between a demo and a deployment.

## Why it exists

The setup bakes the circuit's structure into the keys using secret randomness. That randomness
makes proofs sound *and* small. But whoever knew the randomness could craft a fake proof. So the
security of the whole system reduces to: **was the toxic waste really destroyed?**

## The fix: don't trust one person — run a ceremony

A **multi-party computation (MPC) ceremony** has many independent participants each contribute
randomness in sequence. The setup is secure as long as **at least one** participant was honest and
destroyed their part. You don't have to trust everyone — just that not *all* of them colluded.
That's a dramatically weaker, more realistic assumption.

Two phases:

- **Phase 1 ("powers of tau")** — circuit-independent; reusable across many circuits. Big public
  ceremonies exist that you can build on.
- **Phase 2** — specific to *your* circuit. This is the part you run per deployment.

## What this means for the examples

The examples use:

```bash
xark setup --insecure-dev-mode
```

This derives keys deterministically with **no secrecy at all** — the "toxic waste" is public, so
anyone can forge proofs. That is completely fine for learning and local testing, and **completely
unsafe for anything holding value.** The flag is named to make sure you never ship it by accident.

For a real deployment, xark runs an actual phase-2 ceremony:

```bash
xark ceremony ...
```

with Schnorr proofs-of-knowledge and δ-consistency pairing checks so each contribution is
verifiable. This is one of the places xark takes rigor seriously where prototype tooling just hands
you dev keys — see xark's `docs/trusted-setup.md`.

## The one rule

> **Never deploy a circuit to mainnet with `--insecure-dev-mode` keys.** Use keys from a ceremony
> you trust. If you can't point to the ceremony, treat the proofs as forgeable.

Next: [your ZK journey — where to go from here →](./05-your-zk-journey.md)
