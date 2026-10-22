# RUNBOOK - from zero to a verified proof

The reproducible build and deployment path. If a command here disagrees with an example's
`justfile`, the `justfile` is authoritative.

## 1. Toolchain

| Tool | Version | Install |
|------|---------|---------|
| Rust | stable, `1.95.0` pinned | installed from `rust-toolchain.toml` (xark needs 1.89+) |
| xark CLI and crates | `0.3.0` (sibling checkout) | below |
| `@blueshift-gg/xark-wasm` | `0.3.0` (sibling checkout) | below |
| `wasm-pack` | `0.15.0` | `cargo install wasm-pack --version 0.15.0 --locked` |
| `cargo-build-sbf` | `4.2.0` | `cargo install cargo-build-sbf --version 4.2.0 --locked` |
| Node | 22+ | [nodejs.org](https://nodejs.org) |
| `just` | `1.43.1` | `cargo install just --version 1.43.1 --locked` |
| Anchor, for deploy only | `1.1.2` | `avm install 1.1.2 && avm use 1.1.2` |

No nightly Rust: xark 0.3 compiles circuits with your stable toolchain.

Until xark 0.3.0 is published, keep `xark` (on its `remove-nightly-compiler` branch) and
`xark-examples` as sibling checkouts. The circuit crates depend on it by
`path = ..., version = "0.3.0"`, the programs `[patch]` the generated verifier's `xark-verifier`
to it, and each example's `package.json` takes xark-wasm from it:

```bash
cargo install --path ../xark/crates/cli --locked
wasm-pack build ../xark/crates/wasm --target nodejs --release --out-dir dist/node
npm ci            # one install for every example: the root is an npm workspace
xark doctor
```

When 0.3.0 ships: drop the `path`s and the `[patch.crates-io]` sections, point each `package.json` at
`^0.3.0`, regenerate the lockfiles, and run the full suite.

For devnet deployment, also install the Agave CLI from the
[official Anza instructions](https://docs.anza.xyz/cli/install), then create and fund a wallet:

```bash
solana-keygen new
solana config set --url devnet
solana airdrop 2
```

## 2. One example's pipeline

Each example's `just build` runs the same steps:

```bash
xark build <circuit>                        # Rust → wasm → R1CS: target/xark/<name>/
xark setup <circuit> --insecure-dev-mode --r1cs-form reduced   # DEV keys (see §5)
<prove the demo inputs>                     # the verifier crate's self-test vector
xark export <circuit> --allow-insecure --crate-name <name>   # the verifier crate
xark client <circuit> --out client/         # the typed TypeScript client
cd program && cargo build-sbf --arch v3     # the program, linking the verifier crate
```

`xark export` bundles a real proof as its crate's self-test, so something must prove first: 01
runs `xark prove --inputs '{"power_level": 9001}'`; the others run their circuit crate's
`examples/demo.rs`, which builds the inputs with the circuit's own Rust (a commitment, a Merkle
root) and proves them with `xark::prover`.

Programs build for SBPF v3. The generated verifier crate takes `xark-verifier` with its default
`static-syscalls` feature, which only a v3 program loads: a v0 build fails with "Relative jump out of
bounds". (With that feature off, `xark-verifier` picks the syscall ABI from the target, and v0 and
v3 builds both verify, at the same compute.)

`xark inspect <circuit> --json` lists the public inputs leaf by leaf, in proof order: the order a
program assembles them in. `xark profile <circuit> --html` attributes every constraint to its source line.

## 3. Full repository verification

From the repository root:

```bash
just fmt-check
just test-circuits   # every circuit's own Rust tests
just test            # build everything, then npm test: tsc + every client test, per example
just clippy
just check-clients   # the committed clients are current
```

`just test` is the end-to-end test: each example's client proves in node through xark-wasm, and
the proof goes in a transaction to the example's program in LiteSVM (in-process, with the real
`alt_bn128` syscalls; no validator or network).

## 4. Deploying a program

```bash
# Examples 01 and 02
cd program
cargo build-sbf --arch v3
solana program deploy target/deploy/<name>.so

# Examples 03 and 04
cd program
cargo build-sbf --arch v3
anchor deploy
```

The client tests are the reference clients: exact Anchor discriminators, account metas, PDA seeds,
public-input encoding, and the wallet's bookkeeping of notes and paths.

## 5. Trusted setup

The recipes deliberately use development keys. They are not production-safe and never committed. A
deployment that can hold value needs a reviewed circuit and an auditable multi-party phase-2
ceremony: [docs/ceremony.md](./docs/ceremony.md) walks one through on 01's circuit. Preserve the transcript, and pin the
client to the circuit and key it was made for: `Circuit.prover(url, { digest })` and
`Circuit.verifier(vk, { keyHash })`.

## 6. Integration contracts to preserve

1. **Public-input order.** Each shielded circuit's public statement struct (`Append`, `Withdrawal`,
   `Transaction`) is the order; the program's `public_inputs` must match it. `xark inspect --json`
   lists it leaf by leaf.
2. **One tree definition.** Every circuit root is `xark-merkle`'s `merkle_root`; the wallet's
   `Path` (through the client) is tested against a tree hashed from scratch, and the client tests
   check each program's empty root against `Path.first().treeRoot()`.
3. **Single-writer tree tip.** Deposit and transact proofs bind the current root and append index.
   A wallet or relayer must sequence submissions, refresh pool state immediately before proving,
   and prove again when another append wins the race. A high-throughput product needs a
   sequencer/batcher or an on-chain append design in front of this reference protocol.
4. **Note delivery.** Example 04 binds the withdrawal's token account and both encrypted memos'
   hashes into the proof; the program recomputes them before verifying.
