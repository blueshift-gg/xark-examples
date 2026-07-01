# xark Examples

Zero-knowledge circuits written in [Noir](https://noir-lang.org/) with on-chain verification on
[Solana](https://solana.com/) using [Groth16](https://eprint.iacr.org/2016/260) via
[**xark**](https://github.com/blueshift-gg/xark).

Every example is **built, proved, and verified end-to-end in an in-process Solana VM** — see
[Testing](#testing).

## What's a circuit?

A circuit is a program whose execution you can *prove* without revealing its inputs. You write it
in Noir, mark some inputs `pub` (public) and leave the rest private, and add the constraints that
must hold. xark turns that circuit into a ~256-byte Groth16 proof and a Solana verifier that checks
it through the native `alt_bn128` syscalls — so a program can act on "this statement is true"
without ever seeing the private data behind it.

Brand new to ZK? Start with **[docs/learn-zk](./docs/learn-zk/)** — a from-zero primer.

## Example circuits

Ordered easiest → hardest; each introduces one new idea.

| Example | Proves | New concept | Public inputs | On-chain |
|---------|--------|-------------|:-------------:|----------|
| [01 · over-9000](./01-over-9000/) | a secret value is `> 9000` | range proofs, pure verify | 0 | Pinocchio |
| [02 · age-verification](./02-age-verification/) | `age ≥ 18` without revealing a birthday | commitments | 2 | Pinocchio |
| [03 · shielded-pool](./03-shielded-pool/) | an unlinkable deposit → withdrawal | Merkle membership, nullifiers, 2 circuits | 4 + 7 | Anchor |

## Prerequisites

- [Noir](https://noir-lang.org/docs/getting_started/quick_start) `1.0.0-beta.22` (must match xark's ACIR pin)
- The [`xark`](https://github.com/blueshift-gg/xark) CLI
- Rust `1.85+`, and the [Anza CLI](https://docs.anza.xyz/cli/install) for `cargo build-sbf`
- [`just`](https://github.com/casey/just); Anchor `1.1` for example 03

```bash
# Noir
curl -L https://raw.githubusercontent.com/noir-lang/noirup/main/install | bash
noirup -v 1.0.0-beta.22

# xark (from your xark checkout)
cargo install --path ../xark/crates/cli
```

## Wallet setup

For devnet deployment (the in-VM tests need no wallet):

```bash
solana-keygen new
solana config set --url devnet
solana airdrop 2
```

## Quick start

```bash
cd 01-over-9000
just prove          # nargo execute → xark setup → prove → verify
just export         # xark export → the on-chain verifier crate + instruction_data.bin
just build-program  # cargo build-sbf → the deployable .so
```

Then deploy and submit per the example's README, or run the whole suite in-VM (below).

## Testing

Unlike a devnet round-trip, the [`e2e/`](./e2e/) suite loads each compiled program into
[LiteSVM](https://github.com/LiteSVM/litesvm) and submits a real proof — verifying it through the
same `alt_bn128` syscalls mainnet uses, with **no validator and no network**. Each test also
submits a *tampered* proof and asserts it's rejected; the pool test runs a full
deposit → withdraw and asserts a double-spend fails.

```bash
# build every example first (see each RUNBOOK), then:
cd e2e && cargo test
# over_9000_verifies_on_chain ... ok
# age_verification_verifies_on_chain ... ok
# shielded_pool_full_flow ... ok
```

## Pipeline overview

```
  Noir circuit           you write the rules: what's secret, what's public, what must hold
     │  nargo execute
     ▼
  ACIR + witness         a compiled circuit + a specific satisfying assignment
     │  xark  (lower ACIR → R1CS, Groth16 over BN254)
     ▼
  proof + verifier       a 256-byte proof, and `xark export`s a verifier crate (VK baked in)
     │
     ▼
  Solana program         calls verify_instruction_data(...) via the native alt_bn128 syscalls
```

### Noir (off-chain circuit development)

| Step | Command | Output |
|------|---------|--------|
| Compile + witness | `nargo execute` | `target/<name>.json`, `<name>.gz` |

### xark (proving & verifier creation)

| Step | Command | Output |
|------|---------|--------|
| Inspect | `xark inspect` | opcode coverage, public-input count |
| Setup | `xark setup --insecure-dev-mode` | proving + verifying keys (dev only) |
| Prove | `xark prove` | `proof.bin`, `public_inputs.bin` (+ snarkjs JSON) |
| Verify | `xark verify` | `Proof verified: true` |
| Export | `xark export` | a verifier crate + `instruction_data.bin` |

### Solana (on-chain verification)

| Step | Command | Output |
|------|---------|--------|
| Build | `cargo build-sbf` | `target/deploy/<name>.so` |
| Deploy | `solana program deploy …` | program id |

## On-chain verification

`xark export` generates a self-contained verifier crate with the verifying key embedded at compile
time. Your program depends on it and calls one function:

```rust
// instruction_data = proof (256 B) || public_inputs (N × 32 B, little-endian)
if verifier::verify_instruction_data(instruction_data) {
    // proof valid — act on it
}
```

When the circuit changes, re-run `xark export`; the generated crate is the only thing that updates.

## How xark compares

The Solana Foundation's [`noir-examples`](https://github.com/solana-foundation/noir-examples) use
**Sunspot** for this backend slot. xark is the production-grade alternative: a real multi-party
**MPC ceremony** (`xark ceremony`) instead of dev keys, **Lean formal proofs** + fuzzing +
differential tests against snarkjs, **explicit opcode rejection** (it refuses what it can't prove
soundly), snarkjs-compatible output, and a Pinocchio low-CU path.

## Status & safety

> Reference implementations for learning — **unaudited, not for production.** `03-shielded-pool`
> in particular is educational; privacy tooling carries real legal weight depending on your
> jurisdiction. Never ship `--insecure-dev-mode` keys; use `xark ceremony` for anything real.

## Resources

- [Noir documentation](https://noir-lang.org/docs)
- [xark](https://github.com/blueshift-gg/xark) — `docs/architecture.md`, `docs/trusted-setup.md`, `docs/security.md`
- [Learn ZK, from zero](./docs/learn-zk/)

## License

MIT — see [LICENSE](./LICENSE).
