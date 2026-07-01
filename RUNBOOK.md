# RUNBOOK — from zero to a verified proof on devnet

Everything you need to actually run the examples. If a command here disagrees with an example's
`justfile`, the `justfile` wins (it's what runs).

> All three examples are compiled, proved, and verified end-to-end by the LiteSVM suite in `e2e/`
> (see §5). The items most worth re-confirming against your own toolchain are flagged as
> **⚠ CHECK** below.

## 1. Prerequisites

| Tool | Version | Install |
|------|---------|---------|
| Noir (`nargo`) | **1.0.0-beta.22** | `curl -L noirup.dev \| bash` then `noirup -v 1.0.0-beta.22` |
| `xark` CLI | current | `cargo install --path <your-xark-checkout>/crates/cli` |
| Rust | 1.85+ | `rustup update` (edition 2024 support is required) |
| Anza CLI (`cargo-build-sbf`, `solana`) | latest | https://docs.anza.xyz/cli/install |
| Anchor | 1.1 (example 03 only) | `avm install 1.1.2 && avm use 1.1.2` |
| `just` | any | `cargo install just` |
| `snarkjs` | optional | `npm i -g snarkjs` (browser/JS verification) |

Versions in use: nargo `1.0.0-beta.22`, pinocchio `0.11`, anchor-lang `1.1`, litesvm `0.13`.

Devnet wallet:

```bash
solana-keygen new                       # if you don't have one
solana config set --url devnet
solana airdrop 2
```

## 2. The `xark-verifier` dependency (important, one-time)

`xark export` generates a verifier crate whose `Cargo.toml` says `xark-verifier = "0.1"`. Until
xark is published to crates.io, each program redirects it with a `[patch.crates-io]` block that
**assumes xark is cloned as a sibling of this repo** (`…/GitHub/xark` next to `…/GitHub/xark-examples`):

```toml
[patch.crates-io]
xark-verifier = { path = "../../../xark/crates/verifier" }
```

If your xark checkout lives elsewhere, edit that path in each program's `Cargo.toml`.

## 3. The pipeline, one command at a time

Every example follows the same shape (example 03 does it twice — once per circuit):

```bash
nargo execute                     # compile the circuit + generate the witness from Prover.toml
xark inspect                      # ⚠ CHECK: prints the public-input order — must match the program
xark setup --insecure-dev-mode    # DEV keys only. Real deployments: `xark ceremony` (see §6)
xark prove                        # produce the Groth16 proof
xark verify                       # → "Proof verified: true"
xark export --crate-name <name>   # generate the on-chain verifier crate + instruction_data.bin
```

`just prove` / `just export` wrap these. For 03: `just prove-deposit` and `just prove-withdraw`.

## 4. Build & deploy the program

```bash
# examples 01 / 02 (Pinocchio):
cd program && cargo build-sbf
solana program deploy target/deploy/<name>.so     # note the printed program id

# example 03 (Anchor):
cd program && anchor build && anchor deploy
```

## 5. Verify / submit a proof

To see any example verify on-chain **without a validator**, run the in-VM suite:

```bash
cd e2e && cargo test   # over_9000, age_verification, shielded_pool_full_flow — all pass
```

To deploy for real, `solana program deploy` the `.so` and submit its `instruction_data.bin` (the
exact instruction encodings + account metas for the pool are in `e2e/tests/pool.rs`).

## 6. Trusted setup: dev vs real

`--insecure-dev-mode` derives keys deterministically — anyone can forge proofs. Fine for local
demos, **never** for value. For a real deployment run a multi-party phase-2 ceremony:

```bash
xark ceremony ...        # see xark's docs/trusted-setup.md
```

Ship only keys produced by a ceremony you trust.

## 7. Cross-check in JavaScript (optional, but a great confidence signal)

`xark setup`/`xark prove` also emit snarkjs-compatible JSON, so the same proof verifies outside
Solana:

```bash
cd circuit
snarkjs groth16 verify \
  target/groth16/snarkjs-verification_key.json \
  target/groth16/snarkjs-public.json \
  target/groth16/snarkjs-proof.json
```

## 8. The three **⚠ CHECK**s

1. **Public-input order.** `xark inspect` prints the order the circuit exposes public inputs. It
   must match how each program concatenates them (documented at the top of every `program/src`).
   If they differ, reorder the program's `extend_from_slice` calls.
2. **Poseidon2.** The circuits use `std::hash::poseidon2_permutation` (beta.22 exposes only the
   permutation, not a high-level hash) — the same primitive xark supports as `Poseidon2Permutation`.
   A future nargo that changes this lowering would need the construction revisited.
3. **Off-chain input agreement (example 03).** Any tool that generates the pool's witness inputs
   must reproduce the circuit's `poseidon2_permutation` construction exactly. The e2e test uses
   nargo, so it is consistent by construction.
