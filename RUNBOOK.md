# RUNBOOK - from zero to a verified proof

The reproducible build and deployment path. If a command here disagrees with an example's
`package.json` scripts, the scripts are authoritative: CI runs them.

## 1. Toolchain

| Tool | Version | Install |
|------|---------|---------|
| Rust | stable, `1.95.0` pinned | installed from `rust-toolchain.toml` (xark needs 1.89+) |
| xark CLI | `0.3` | `cargo install xark-cli --version 0.3.0 --locked` |
| xark crates, `@blueshift-gg/xark` | `0.3` | from crates.io and npm, through the manifests |
| `cargo-build-sbf` | `4.2.0` | `cargo install cargo-build-sbf --version 4.2.0 --locked` |
| Node | 22+ | [nodejs.org](https://nodejs.org) |

No nightly Rust: xark 0.3 compiles circuits with your stable toolchain.

```bash
cargo install xark-cli --version 0.3.0 --locked
npm ci            # one install for every example: the root is an npm workspace
xark doctor
```

For devnet deployment, also install the Agave CLI from the
[official Anza instructions](https://docs.anza.xyz/cli/install), then create and fund a wallet:

```bash
solana-keygen new
solana config set --url devnet
solana airdrop 2
```

## 2. One example's pipeline

Each example's `npm run build` runs the same steps:

```bash
xark build <circuit>                        # Rust → wasm → R1CS: target/xark/<name>/
xark setup <circuit> --r1cs-form reduced   # a single-party key (see §5)
<prove the demo inputs>                     # the verifier crate's self-test vector
xark export <circuit> --crate-name <name>   # the verifier crate, beside the circuit's sources
xark client <circuit> --out client/         # the typed TypeScript client
cd program && cargo build-sbf --arch v3     # the program, linking the verifier crate
```

`cargo build-sbf` only warns when a function's stack frame passes SBF's 4 KiB ("Function … overflows
the maximum allowed frame space"), and at run time that frame silently corrupts the stack, so
`build:program` fails on that warning. It touches `src/lib.rs` first, so the program links again and
the warning, which cargo doesn't replay from its cache, is seen on every build.

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
npm ci
npm run build           # every example: circuits, keys, verifier crates, clients, programs
npm run check:clients   # the committed clients are the ones the build generated
npm run test:circuit    # every circuit's own Rust tests, then the programs' host tests
npm test                # tsc + every client test, per example
npm run fmt:check       # after the build: the programs' manifests name the verifier crates
npm run clippy
```

Each runs the script of the same name in every example, in order; `cd <example>` and the same
command runs just that one. CI ([`ci.yml`](./.github/workflows/ci.yml)) runs these steps in each
example's directory, one job per example.

`npm test` is the end-to-end test: each example's client proves in node through `@blueshift-gg/xark`, and
the proof goes in a transaction to the example's program in LiteSVM (in-process, with the real
`alt_bn128` syscalls; no validator or network).

## 4. Deploying a program

Every program is Pinocchio: one `.so`, with no framework and no IDL. After the example's
`npm run build`, which generates the verifier crates the program links:

```bash
cd <example>/program
cargo build-sbf --arch v3                   # target/deploy/<name>.so and <name>-keypair.json
solana program deploy target/deploy/<name>.so
```

The first deploy takes its program address from `target/deploy/<name>-keypair.json`, which
`cargo build-sbf` makes if it's missing; keep that keypair (it's ignored by git) to upgrade the same
program later. The programs that own accounts (03 to 06) name their own address, `ID` in
`program/src/lib.rs`, as Pinocchio programs do, and derive their PDAs and check owners against it: to
deploy one elsewhere, put that keypair's address in `ID` and in the client, and build again. 01 and
02 own no accounts and run at any address. The client tests load each program at its `ID` in
LiteSVM (01 and 02 at an address of their own).

The client tests are the reference clients: each instruction's tag and data layout, account metas,
PDA seeds, public-input encoding, and the wallet's bookkeeping of notes and paths.

## 5. Trusted setup

The recipes make single-party keys from the machine's randomness, and never commit them. Such a key
is sound unless whoever ran setup kept that randomness, but users of the verifier have to trust that
one party; the verifier crate's `SETUP` says `single-party`. A deployment that holds value needs a
reviewed circuit and an auditable multi-party phase-2 ceremony (`SETUP` is then `ceremony`): [docs/ceremony.md](./docs/ceremony.md) walks one through on 01's circuit. Preserve the transcript, and pin the
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
   sequencer/batcher or an onchain append design in front of this reference protocol.
4. **Note delivery.** Example 06 binds the withdrawal's token account and both encrypted memos'
   hashes into the proof; the program recomputes them before verifying.
