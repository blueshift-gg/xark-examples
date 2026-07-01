# xark docs

Two ways in, depending on where you're starting.

## I've never touched zero-knowledge → **[Learn ZK, from zero](./learn-zk/)**

A short, honest track that gives you the mental model before any Noir. Five pages, ~30 minutes:

1. [What is a ZK proof?](./learn-zk/01-what-is-a-zk-proof.md) — the one idea, no maths
2. [Circuits & constraints](./learn-zk/02-circuits-and-constraints.md) — how "code" becomes "proof"
3. [Groth16 in one page](./learn-zk/03-groth16-in-one-page.md) — the proof system these examples use
4. [Trusted setup, intuitively](./learn-zk/04-trusted-setup-intuition.md) — what the ceremony buys you
5. [Your ZK journey](./learn-zk/05-your-zk-journey.md) — the concrete path from here, curated links

## I know ZK, show me xark → **[the examples](../README.md)**

Jump straight to the [gallery](../README.md) and run [01-over-9000](../01-over-9000/). The
[RUNBOOK](../RUNBOOK.md) has the full toolchain.

## The xark pipeline in one picture

Every example — and every private app you'll build — is this loop:

```
  Noir source            you write the rules: what's secret, what's public, what must hold
     │  nargo compile / nargo execute
     ▼
  ACIR + witness         a compiled circuit, plus a specific secret assignment that satisfies it
     │  xark  (lower ACIR → R1CS, Groth16 over BN254)
     ▼
  proof + verifier       a 256-byte proof, and — via `xark export` — a verifier crate with the
     │                    verifying key baked in
     ▼
  Solana program         calls `verify_instruction_data(...)` through the native alt_bn128 syscalls
```

Noir is the frontend (the language). **xark is the backend** (proving + the on-chain verifier).
You never leave Noir to change the circuit; you re-run `xark export` and your program is untouched.
