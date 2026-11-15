# Contributing

## Setup

Install the toolchain from `RUNBOOK.md`, then run:

```bash
npm ci
xark doctor
npm run build       # every example's xark build
npm run check       # every example's xark build --check
npm test            # every example's xark test
npm run fmt:check
npm run clippy
```

`npm run build` makes each example's circuits, keys, clients and SBF program; `npm test` runs the
circuits' and programs' Rust tests, then proves with each client and verifies onchain in LiteSVM.
From an example's directory, `xark build` and `xark test` check that example alone, as its CI job
does. Commit what `xark build` writes in `keys/` and `clients/` (the verifier crate embeds the
key); never commit real witness data.

## Repository contracts

- One source per fact. Commitments, nullifiers, roots and paths are computed by the circuits' own
  Rust: the client tests call it through the generated clients, and the demo proofs through the
  crate. Don't reimplement them in TypeScript or in a program. The client tests check the
  programs' empty-root constants against the circuits' `Path.first().treeRoot()`.
- The keys and generated clients are committed so readers see them and fresh clones build the same
  verifier; `xark build --check` fails if one is stale. Rebuild (`xark build`) and commit the
  result with the circuit change.
- Treat a circuit's public statement struct (`Append`, `Withdrawal`, `Transaction`) as an API: the
  program fills it in as the generated `PublicInputs`. A change updates the struct, the program, and
  the README reference together.
- Each example is its own xark project (its `xark.toml`, workspace, `package.json`, `tsconfig.json`
  and `vitest.config.ts`), so one can be copied out alone. The tests' helpers repeat on purpose;
  keep each to what its example uses.
- `xark build` fails if `cargo build-sbf` reports a function whose stack frame passes SBF's
  4 KiB, which it only warns about: at run time such a frame silently corrupts the stack (it broke
  token claim's claims, and 01's entrypoint once had a 4,480-byte frame). Keep large buffers
  borrowed from the instruction data, or put a heavy function out of line with `#[inline(never)]`.
- Preserve the explicit dev-key warnings. Production ceremony artifacts do not belong in example
  pull requests.

## Pull requests

Keep changes scoped to one protocol or tooling concern. Explain any change to a circuit invariant,
public input, trusted assumption, PDA seed, or serialization format. Include the exact verification
commands you ran and call out anything the local environment could not execute.
