# RUNBOOK - from zero to a verified proof

This is the reproducible build and deployment path for the repository. If a command disagrees with
an example's `justfile`, the `justfile` is authoritative.

## 1. Pinned toolchain

| Tool | Version | Install |
|------|---------|---------|
| xark nightly | `nightly-2026-05-03` | `rustup toolchain install nightly-2026-05-03 --profile minimal --component rust-src --component rustc-dev --component llvm-tools` |
| xark CLI and crates | commit `44f9a8d3b1f5af90a57249fa1dad308968922a03` | command below |
| Rust | `1.95.0` | installed from `rust-toolchain.toml` |
| `cargo-build-sbf` | `4.0.0` | `cargo install cargo-build-sbf --version 4.0.0 --locked` |
| Anchor, for deploy only | `1.1.2` | `avm install 1.1.2 && avm use 1.1.2` |
| `just` | `1.43.1` | `cargo install just --version 1.43.1 --locked` |
| `snarkjs`, optional | current | `npm install --global snarkjs` |

Install the same xark revision used by every circuit and generated verifier:

```bash
cargo +nightly-2026-05-03 install \
  --git https://github.com/blueshift-gg/xark \
  --rev 44f9a8d3b1f5af90a57249fa1dad308968922a03 \
  xark --locked --features cli
```

Run `xark doctor` after installation. The repository commits every Cargo lockfile needed by its
standalone circuit, program, and E2E crates.

For devnet deployment, also install the Agave CLI from the
[official Anza instructions](https://docs.anza.xyz/cli/install), then create and fund a development
wallet:

```bash
solana-keygen new
solana config set --url devnet
solana airdrop 2
```

## 2. How the repository depends on xark

There are three version-aligned surfaces:

1. Each circuit pins `xark` and gadget crates to the commit above.
2. The installed CLI is built from that same commit.
3. Every `xark export` command writes an exact-revision `xark-verifier` dependency into the generated
   verifier crate. Program workspaces patch that dependency to the checked-in
   `vendor/xark-verifier` snapshot of the same commit, whose only manifest change disables the
   dependency's incompatible static-syscalls default.

No sibling xark checkout or generated-manifest rewrite is required. `vendor/xark-verifier/UPSTREAM.md`
records the compatibility patch. Updating xark is a deliberate repository-wide operation: update
the revision in circuit manifests, justfiles, CI, the vendor snapshot, and this runbook together;
regenerate lockfiles; then run the full suite.

## 3. The circuit pipeline

The underlying commands are:

```bash
xark build <circuit-dir>
xark inspect <circuit-dir>
xark setup <circuit-dir>
xark prove <circuit-dir> --input-file <private-input-file>
xark export <circuit-dir> --allow-insecure --crate-name <name> \
  --verifier-dep '{ git = "https://github.com/blueshift-gg/xark", rev = "44f9a8d3b1f5af90a57249fa1dad308968922a03" }'
```

`xark prove` self-verifies the proof, so a second `xark verify` is optional. The justfiles create
mode-`0600` temporary input files with `mktemp` rather than exposing private witness values in process
arguments. Setup without a `.ptau` produces a dev key; `--allow-insecure` is intentionally required
to export it.

For one example, use its complete target instead of running every intermediate target in sequence:

```bash
cd 01-over-9000
just build-program
```

## 4. Full repository verification

From the repository root:

```bash
just fmt-check
just check-circuits
just test-circuits
just test
just clippy
```

`just test` builds all circuits, dev keys, proofs, generated verifier crates, and Solana programs,
then runs the LiteSVM suite through mainnet-feature `alt_bn128` syscalls. It needs no validator or
network after dependencies and SBF platform tools are installed.

## 5. Deploying a program

```bash
# Examples 01 and 02
cd program
cargo build-sbf
solana program deploy target/deploy/<name>.so

# Examples 03 and 04
cd program
cargo build-sbf
anchor deploy
```

The E2E tests are the reference instruction clients. They show the exact Anchor discriminators,
account metas, proof/public-input encoding, PDA seeds, and state checks used by the programs.

## 6. Trusted setup

The checked-in recipes deliberately use development setup keys. Those keys are not production-safe
and are never committed. A deployment that can hold value needs a reviewed circuit and an auditable
multi-party phase-2 ceremony:

```bash
xark ceremony ...
```

See xark's `docs/trusted-setup.md`. Preserve the ceremony transcript and pin the resulting verifier
artifact to the reviewed circuit hash.

## 7. JavaScript cross-check

xark emits snarkjs-compatible JSON beside its binary artifacts:

```bash
snarkjs groth16 verify \
  target/xark/<name>/snarkjs-verification_key.json \
  target/xark/<name>/snarkjs-public.json \
  target/xark/<name>/snarkjs-proof.json
```

## 8. Integration contracts to preserve

1. **Public-input order.** The circuit declaration, generated verifier, program calldata, witness
   generator, and client must agree. Confirm changes with `xark inspect`.
2. **Poseidon2 identity.** `e2e/src/poseidon2.rs` is generated from the pinned gadget source and
   checked against its known-answer vector. Regenerate it with `scripts/gen_poseidon2_native.py`
   when updating xark.
3. **Nonzero empty leaf.** Both shielded examples use the transparent `xarkzero` field constant and
   the same generated zero-subtree table and on-chain empty root.
4. **Single-writer tree tip.** Deposit and transact proofs bind the current root and append index.
   A wallet or relayer must sequence submissions, refresh pool state immediately before proving,
   and regenerate a proof rejected after another append wins the race. A high-throughput product
   should put a sequencer/batcher or an on-chain append design in front of this reference protocol.
5. **Note delivery.** Example 04 binds the full withdrawal token-account address and SHA-256 hashes
   of both encrypted memo envelopes into the proof. The program recomputes those hashes before
   verification.
