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

Each example's `npm run build` runs:

```bash
xark setup <circuit-dir> --r1cs-form reduced
```

With no `.ptau`, that's a single-party setup from the machine's randomness. (`--r1cs-form reduced`
picks the R1CS form the provers stream from the circuit itself.) It's sound unless whoever
ran it kept that randomness, which xark never writes down, but everyone relying on the verifier has
to trust that one party, and there's no transcript to show them. `xark export` records how a key was
made in the verifier crate's `SETUP` constant: `single-party` here, `ceremony` for two or more
contributors, and `seeded` for a test key made with `--deterministic-rng`, which anyone with the seed
can forge proofs for.

For a real deployment, xark runs an actual phase-2 ceremony (`xark ceremony init`, `contribute`,
`verify`, `finalize`; [docs/ceremony.md](../ceremony.md) walks one through on example 01), with
Schnorr proofs-of-knowledge and δ-consistency pairing checks so each contribution is verifiable. One
honest contributor is enough, so no single party has to be trusted — see xark's
`docs/trusted-setup.md`.

## The one rule

> **Know whom your key asks users to trust.** A single-party key asks them to trust whoever ran
> setup; for anything holding value, use keys from a ceremony. Never deploy a seeded key.

Next: [your ZK journey — where to go from here →](./05-your-zk-journey.md)
