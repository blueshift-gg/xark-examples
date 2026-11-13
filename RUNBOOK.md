# RUNBOOK - from zero to a verified proof

The reproducible build and deployment path. If a command here disagrees with an example's
`package.json` scripts, the scripts are authoritative: CI runs them.

## 1. Toolchain

| Tool | Version | Install |
|------|---------|---------|
| Rust | stable, `1.95.0` pinned | installed from `rust-toolchain.toml` (xark needs 1.89+) |
| xark CLI | `0.4` | `cargo install xark-cli --version 0.4.0 --locked` |
| xark crates, `@blueshift-gg/xark` | `0.4` | from crates.io and npm, through the manifests |
| `cargo-build-sbf` | `4.2.0` | `cargo install cargo-build-sbf --version 4.2.0 --locked` |
| Node | 22+ | [nodejs.org](https://nodejs.org) |

No nightly Rust: xark 0.4 compiles circuits with your stable toolchain.

```bash
cargo install xark-cli --version 0.4.0 --locked
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

## 2. One example

Each example is an xark project (`xark init` makes one): a `xark.toml`, one Cargo workspace, its
circuits in `circuits/`, its program in `programs/`, the clients xark generates in `clients/`, its
keys in `keys/`, and its LiteSVM tests in `tests/`. In its directory:

```bash
xark build      # every circuit, its keys if missing, its clients (clients/<circuit>/{rust,ts}), then the program
xark test       # the circuits' tests, the program's and verifier crates' Rust tests, then the LiteSVM tests
```

`xark build` fails when a stack frame passes SBF's 4 KiB, which `cargo build-sbf` only warns about
("Function … overflows the maximum allowed frame space") and which silently corrupts the stack at
run time.

The keys and the generated clients are committed: the verifier crate embeds the verifying key, and
a key only in `target/` would be replaced on every fresh clone. Each key keeps the `circuit.xbc` it
was made from beside it, since a key proves only with that build. `xark build --check` (CI) fails when
they aren't what the source builds. The verifier crates bundle a sample proof as their self-test:
the circuits' `examples/demo.rs` write one (built from the circuit's own Rust: a commitment, a
Merkle root), and a rebuild keeps it while it still verifies.

The verifier crate's syscall ABI follows the target, so a program builds for `cargo build-sbf`'s
default (SBPF v0) or for v3 (`--arch v3`), and verifies at the same compute either way.

A program builds its public inputs as the generated crate's `PublicInputs`, by name, and `verify`
packs them as the circuit does. `xark inspect <circuit> --json` lists them leaf by leaf, in proof
order, and `xark profile <circuit> --html` attributes every constraint to its source line.

## 3. Full repository verification

From the repository root:

```bash
npm ci
npm run build           # every example's xark build
npm run check           # every example's xark build --check: committed keys and clients are current
npm test                # every example's xark test
npm run fmt:check
npm run clippy
```

Each runs the script of the same name in every example, in order; `cd <example>` and the same
command runs just that one. CI ([`ci.yml`](./.github/workflows/ci.yml)) runs `xark build --check`,
`xark test`, `cargo fmt` and `cargo clippy` in each example's directory, one job per example.

The LiteSVM tests are the end-to-end tests: each example's client proves in node through
`@blueshift-gg/xark`, and the proof goes in a transaction to the example's program in LiteSVM
(in-process, with the real `alt_bn128` syscalls; no validator or network).

## 4. Deploying a program

Every program is Pinocchio: one `.so`, with no framework and no IDL. After the example's
`xark build`:

```bash
cd <example>
cargo build-sbf                             # target/deploy/<name>.so and <name>-keypair.json
solana program deploy target/deploy/<name>.so
```

The first deploy takes its program address from `target/deploy/<name>-keypair.json`, which
`cargo build-sbf` makes if it's missing; keep that keypair (it's ignored by git) to upgrade the same
program later. The programs that own accounts (03 to 06) name their own address, `ID` in
`programs/<name>/src/lib.rs`, as Pinocchio programs do, and derive their PDAs and check owners
against it: to deploy one elsewhere, put that keypair's address in `ID` and in the client, and build
again. 01 and 02 own no accounts and run at any address. The LiteSVM tests load each program at its
`ID` (01 and 02 at an address of their own).

The LiteSVM tests are the reference clients: each instruction's tag and data layout, account metas,
PDA seeds, public-input encoding, and the wallet's bookkeeping of notes and paths.

## 5. Trusted setup

The examples' keys are single-party keys from the machine's randomness, committed in `keys/`. Such a key
is sound unless whoever ran setup kept that randomness, but users of the verifier have to trust that
one party; the verifier crate's `SETUP` says `single-party`. A deployment that holds value needs a
reviewed circuit and an auditable multi-party phase-2 ceremony (`SETUP` is then `ceremony`): [docs/ceremony.md](./docs/ceremony.md) walks one through on 01's circuit. Preserve the transcript, and pin the
client to the circuit and key it was made for: `Circuit.prover(url, { digest })` and
`Circuit.verifier(vk, { keyHash })`.

## 6. Integration contracts to preserve

1. **Public inputs.** Each circuit's public statement (`Append`, `Withdrawal`, `Transaction`, …) is
   what the program fills in, as the generated crate's `PublicInputs`; the crate packs it in the
   circuit's order: a reordered field needs nothing from the program, and a renamed or removed
   one is a compile error there, not a proof that fails.
2. **One tree definition.** Every tree is `xark-merkle`'s Poseidon2 tree: every circuit root is its
   `merkle_root` (an append its `merkle_append`), the wallet's `Path` (through the client) is tested
   against `xark_merkle::tree`'s builder, and the client tests check each program's empty root
   against `Path.first().treeRoot()`.
3. **Single-writer tree tip.** Deposit and transact proofs bind the current root and append index.
   A wallet or relayer must sequence submissions, refresh pool state immediately before proving,
   and prove again when another append wins the race. A high-throughput product needs a
   sequencer/batcher or an onchain append design in front of this reference protocol.
4. **Note delivery.** Example 06 binds the withdrawal's token account and both encrypted memos'
   hashes into the proof; the program recomputes them before verifying.
