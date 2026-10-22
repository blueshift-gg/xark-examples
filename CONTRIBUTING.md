# Contributing

## Setup

Install the toolchain from `RUNBOOK.md`, then run:

```bash
xark doctor
just fmt-check
just test-circuits
just test
just clippy
just check-clients
```

`just test` makes development keys, verifier crates, clients and SBF programs under ignored
`target/` directories, then proves with each client and verifies on-chain in LiteSVM. Never commit
generated keys or real witness data.

## Repository contracts

- One source per fact. Commitments, nullifiers, roots and paths are computed by the circuits' own
  Rust: the client tests call it through the generated clients, and the demo proofs through the
  crate. Don't reimplement them in TypeScript or in a program. The client tests check the
  programs' empty-root constants against the circuits' `Path.first().treeRoot()`.
- The generated clients are committed so readers see them; `just check-clients` fails if one is
  stale. Rebuild (`just build`) and commit the result with the circuit change.
- Treat public-input order as an API: it's each circuit's public statement struct (`Append`,
  `Withdrawal`, `Transaction`). A change updates the struct, the program's `public_inputs`, and the
  README reference together.
- Each example is its own project (its `package.json`, `tsconfig.json`, `vitest.config.ts` and
  `client/harness.ts`), so one can be copied out alone. The harnesses repeat on purpose; keep
  each to what its example uses.
- Programs build with `cargo build-sbf --arch v3`. The generated verifier crates take
  `xark-verifier` with its default `static-syscalls` feature, which only a v3 program loads; a v0
  build of it fails with "Relative jump out of bounds".
- Preserve the explicit dev-key warnings. Production ceremony artifacts do not belong in example
  pull requests.

## Pull requests

Keep changes scoped to one protocol or tooling concern. Explain any change to a circuit invariant,
public input, trusted assumption, PDA seed, or serialization format. Include the exact verification
commands you ran and call out anything the local environment could not execute.
