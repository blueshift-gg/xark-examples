# Contributing

## Setup

Install the pinned toolchain from `RUNBOOK.md`, then run:

```bash
xark doctor
just fmt-check
just check-circuits
just test-circuits
just test
just clippy
```

`just test` generates development proving keys, proofs, verifier crates, and SBF artifacts under
ignored `target/` and `chain/` directories. Never commit generated keys or real witness data.

## Repository contracts

- Keep the xark CLI, circuit crates, gadget crates, generated verifier dependency, CI revision, and
  lockfiles on the same exact commit.
- Keep `vendor/xark-verifier/src` byte-identical to the commit recorded in its `UPSTREAM.md`; its
  standalone manifest may carry only the documented dynamic-syscall compatibility change.
- Treat public-input order as an API. A change must update the circuit, program calldata, witness
  generator, E2E instruction builder, README reference, and expected input count together.
- Keep private witness values out of command-line arguments. Generate a mode-`0600` temporary file
  and use `xark prove --input-file`.
- Regenerate `e2e/src/poseidon2.rs`, the Merkle zero table, and both on-chain empty-root constants if
  their pinned upstream definitions change.
- Preserve the explicit dev-key warnings. Production ceremony artifacts do not belong in example
  pull requests.

## Pull requests

Keep changes scoped to one protocol or tooling concern. Explain any change to a circuit invariant,
public input, trusted assumption, PDA seed, or serialization format. Include the exact verification
commands you ran and call out anything the local environment could not execute.
