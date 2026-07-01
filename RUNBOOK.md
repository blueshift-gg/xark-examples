# RUNBOOK — from zero to a verified proof on devnet

Everything you need to actually run the examples. If a command here disagrees with an example's
`justfile`, the `justfile` wins (it's what runs).

> Honesty note: these examples were authored against xark's real APIs and conventions but have
> **not been compiled or deployed in this environment**. Expect to fix the occasional Noir/Anchor
> syntax nit on first build. The three things most worth verifying against your live toolchain are
> flagged as **⚠ CHECK** below.

## 1. Prerequisites

| Tool | Version | Install |
|------|---------|---------|
| Noir (`nargo`) | **1.0.0-beta.22** | `curl -L noirup.dev \| bash` then `noirup -v 1.0.0-beta.22` |
| `xark` CLI | current | `cargo install --path <your-xark-checkout>/crates/cli` |
| Rust | 1.85+ | `rustup update` (edition 2024 support is required) |
| Anza CLI (`cargo-build-sbf`, `solana`) | latest | https://docs.anza.xyz/cli/install |
| Anchor | 0.31.1 (example 03 only) | `avm install 0.31.1 && avm use 0.31.1` |
| Node | 20+ (clients) | any |
| `just` | any | `cargo install just` |
| `snarkjs` | optional | `npm i -g snarkjs` (browser/JS verification) |

Devnet wallet:

```bash
solana-keygen new                       # if you don't have one
solana config set --url devnet
solana airdrop 2
```

## 2. The `xark-verifier` dependency (important, one-time)

`xark export` generates a verifier crate whose `Cargo.toml` says `xark-verifier = "0.1"`. Until
xark is published to crates.io, point that at your local checkout. Two ways:

- **Per generated crate:** edit `circuit/target/<name>-xark-verifier/Cargo.toml`, replace
  `xark-verifier = "0.1"` with `xark-verifier = { path = "/abs/path/to/xark/crates/verifier" }`.
- **Workspace patch (example 03):** uncomment the `[patch.crates-io]` block in
  `03-shielded-pool/program/Cargo.toml` and set the path.

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

## 5. Submit a proof

```bash
# examples 01 / 02:
cd client && npm install
PROGRAM_ID=<id> npm run submit

# example 03: use prepare-deposit / prepare-withdraw (writes Prover.toml + prints the
# instruction args), prove, then submit via your Anchor client with the generated IDL.
```

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
2. **Poseidon2 lowering.** The circuits use `std::hash::poseidon2::Poseidon2::hash`. Confirm it
   lowers to the `Poseidon2Permutation` black-box on your nargo (it does on beta.22). If a future
   nargo changes this, switch to a permutation-based compression.
3. **Client/circuit hash agreement (example 03).** The bb.js Poseidon2 in the client must equal the
   circuit's. Pin `@aztec/bb.js` to your nargo's Barretenberg. Quick check: hash `[1, 2]` in both
   and compare.
