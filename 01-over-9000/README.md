# 01 · Over 9000 — a range proof

Prove you know a secret number greater than 9000, revealing nothing else. It's the smallest circuit
that still does something real, and the range-proof mechanism under every later example.

## The circuit

```rust
use xark::prelude::*;

#[circuit]
pub fn circuit(power_level: Private<Field>) {
    assert(power_level > 9000u64);
}
```

A few lines, three ideas:

- **`Private` means witness.** `power_level` is an input the proof is computed from but never
  reveals. The verifier ends up convinced the assertion held without learning the number.
- **The width is a constraint.** A `Field` wraps around, so "greater than" has no meaning until you
  fix a range — in xark the *comparison operand's type* supplies it: `> 9000u64` range-checks
  `power_level` into `[0, 2⁶⁴)` (a 64-bit decomposition in R1CS) and only then orders it. Every
  range proof — balances, ages, prices — is this one trick.
- **Zero public inputs.** `9000` is a literal, so it is fixed inside the verifying key. Only the
  256-byte proof reaches the chain.

It's ordinary Rust: `xark build` type-checks it with rustc itself, extracts the MIR, and lowers it
to constraints. `xark check` surfaces anything outside the provable subset as a normal compiler
diagnostic, in your editor.

## What it does and doesn't prove

It proves someone knows *a* value `> 9000`. It does not tie that value to anything — a prover just
picks 9001. A bare range proof is a mechanism, not an application: bind the hidden value to a
published commitment ([02](../02-age-verification/)) or a Merkle tree of deposits
([03](../03-shielded-pool/)) and the same check becomes an age gate or a private balance.

## Run it

```bash
just build-program  # build → dev setup → prove/self-check → export → deployable .so
cd ../e2e && cargo test over_9000   # verify the proof in a real Solana VM
just test-circuit   # proving 9001 succeeds; proving 9000 fails
```

Prerequisites and toolchain versions: [../RUNBOOK.md](../RUNBOOK.md) (`xark doctor` checks them).

## Experiments

- `just test-circuit` runs the in-crate tests: proving with 9001 succeeds, with 9000 it must fail -
  no witness satisfies the circuit. That failure *is* the guarantee: you cannot prove a false
  statement. Put `power_level = 9000` in an input file and pass it with `--input-file` for the CLI
  version.
- `just verify-snarkjs` checks the same proof in JavaScript. xark's artifacts are snarkjs-compatible,
  so one proof verifies in both a Solana program and a browser. (`xark client circuit` scaffolds a
  TypeScript client around the same files.)

## Layout

```
circuit/   the circuit crate (src/lib.rs — plain Rust, plus its prove-tests)
program/   Pinocchio verifier — one verify_instruction_data call, minimal CU
justfile   prove · export · build-program
```

Next → [02 · Age verification](../02-age-verification/)
