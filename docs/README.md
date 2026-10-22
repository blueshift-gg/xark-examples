# xark docs

Two ways in, depending on where you're starting.

## I've never touched zero-knowledge -> **[Learn ZK, from zero](./learn-zk/)**

A short track that gives you the mental model before any circuit code. Five pages, about 30 minutes:

1. [What is a ZK proof?](./learn-zk/01-what-is-a-zk-proof.md) - the one idea, no maths
2. [Circuits & constraints](./learn-zk/02-circuits-and-constraints.md) - how Rust becomes constraints
3. [Groth16 in one page](./learn-zk/03-groth16-in-one-page.md) - the proof system these examples use
4. [Trusted setup, intuitively](./learn-zk/04-trusted-setup-intuition.md) - what the ceremony buys you
5. [Your ZK journey](./learn-zk/05-your-zk-journey.md) - the concrete path from here

## I know ZK, show me xark -> **[the examples](../README.md)**

Jump straight to the [gallery](../README.md) and run [01-over-9000](../01-over-9000/). The
[RUNBOOK](../RUNBOOK.md) has the pinned toolchain and complete build flow.

## The xark pipeline in one picture

Every example follows the same loop:

```
  Rust circuit          ordinary Rust using typed Private/Public inputs and require* constraints
     | xark build       stable rustc -> wasm -> xark-IR -> R1CS
     v
  circuit + keys       circuit.xbc; xark setup makes the Groth16 keys
     | xark export      a Rust verifier crate, the verifying key baked in
     | xark client      a typed TypeScript client
     v
  app (TypeScript)     prover.prove(inputs) -> a 256-byte proof, sent in a transaction
     v
  Solana program       verify(proof, public inputs) through native alt_bn128 syscalls
```

xark owns the complete circuit toolchain: Rust subset checking, lowering to R1CS, witness solving,
Groth16 setup and proving (in Rust and in wasm), verifier export, and client generation. The application still owns the
public/private contract, wallet-side witness data, trusted-setup ceremony, and on-chain state
transition around the generated verifier.
