# xark-examples

Real, runnable examples of **private applications on Solana**, built with
[Noir](https://noir-lang.org) for the circuits and
[**xark**](https://github.com/blueshift-gg/xark) for proving and on-chain verification.

> xark is a Rust **Groth16 backend for Noir** that verifies proofs on Solana through the
> native `alt_bn128` syscalls. Noir stays the frontend; xark owns the backend: it lowers ACIR
> to R1CS, runs Groth16 over BN254, and `xark export`s a self-contained verifier crate you drop
> straight into a Solana program.

## Why this repo exists

Zero-knowledge is how you get **privacy and verifiable computation** onchain — hide the inputs,
prove the result. The tooling has been the hard part. These examples show the whole path,
end to end, with nothing hand-waved:

```
Noir circuit  ──nargo──▶  ACIR + witness  ──xark──▶  Groth16 proof + verifier crate  ──▶  Solana
```

New to ZK entirely? Start with **[docs/learn-zk](./docs/learn-zk/)** — a from-zero track that
gets you the mental model before a single line of Noir.

## The examples — a learning ladder

Each example is self-contained (`circuit/`, `program/`, `client/`, a `justfile`, and a `README`)
and gets harder in exactly one dimension at a time.

| # | Example | What you prove | New concept | On-chain |
|---|---------|----------------|-------------|----------|
| [01](./01-over-9000/) | **Over 9000** | "my secret power level is `> 9000`" | range proofs · zero public inputs · pure verify | Pinocchio, minimal CU |
| [02](./02-age-verification/) | **Age verification** | "I'm `≥ 18`" — without revealing my birthday | commitments · public inputs · identity | Pinocchio + a public-input check |
| [03](./03-shielded-pool/) | **Shielded pool** | "I deposited, and I'm withdrawing — unlinkably" | Merkle membership · nullifiers · two circuits · app state | Anchor program |

Difficulty and "wow" both climb 01 → 03. Read them in order; each README assumes the one before it.

## How xark compares

The Solana Foundation's [`noir-examples`](https://github.com/solana-foundation/noir-examples) use
**Sunspot** for the same backend slot. xark is the production-grade alternative:

| | Sunspot | xark |
|---|---|---|
| Trusted setup | dev-mode keys | real multi-party **MPC ceremony** (`xark ceremony`) |
| Assurance | — | **Lean formal proofs**, Kani, fuzzing, differential tests vs snarkjs |
| Safety | — | **explicit opcode rejection** — refuses what it can't prove soundly |
| DX | — | `xark export` → verifier crate with the VK baked in; your program never changes |
| Interop | — | snarkjs-compatible JSON (verify in a browser, reuse circom tooling) |

## Quick start

```bash
# Prerequisites (see each example's README for exact versions):
#   - Noir / nargo 1.0.0-beta.22   https://noir-lang.org/docs/installation
#   - xark CLI                     cargo install --path <path-to-xark>/crates/cli
#   - Rust 1.85+, and (for 03) Anchor + the Anza CLI (cargo-build-sbf)

cd 01-over-9000
just prove        # nargo execute → xark setup → xark prove → xark verify
just export       # xark export → generates the on-chain verifier crate
# then follow the example README to deploy + submit on devnet
```

## Status & safety

> **These are reference implementations for learning. They are not audited and not for
> production.** `03-shielded-pool` in particular is educational: privacy tooling carries real
> legal and regulatory weight depending on your jurisdiction — understand it before deploying
> anything derived from it. Never ship keys produced with `xark setup --insecure-dev-mode`;
> use `xark ceremony` for anything real.

## License

MIT — see [LICENSE](./LICENSE).
