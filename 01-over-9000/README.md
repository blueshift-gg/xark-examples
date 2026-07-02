# 01 · Over 9000 — a range proof

Prove you know a secret number greater than 9000, revealing nothing else. It's the smallest circuit
that still does something real, and the range-proof mechanism under every later example.

## The circuit

```noir
fn main(power_level: u64) {
    assert(power_level > 9000);
}
```

Four lines, three ideas:

- **No `pub` means private.** `power_level` is the *witness* — an input the proof is computed from
  but never reveals. The verifier ends up convinced the assertion held without learning the number.
- **The type is a constraint.** `u64` forces `power_level` into `[0, 2⁶⁴)`, and that bound is what
  makes `>` sound: field elements wrap around, so without a range "greater than" has no meaning. `>`
  lowers to ACIR's `RANGE` opcode, which xark compiles to R1CS and proves. Every range proof —
  balances, ages, prices — is this one trick.
- **Zero public inputs.** `9000` is a literal, so it is fixed inside the verifying key. Only the
  256-byte proof reaches the chain.

## What it does and doesn't prove

It proves someone knows *a* value `> 9000`. It does not tie that value to anything — a prover just
picks 9001. A bare range proof is a mechanism, not an application: bind the hidden value to a
published commitment ([02](../02-age-verification/)) or a Merkle tree of deposits
([03](../03-shielded-pool/)) and the same check becomes an age gate or a private balance.

## Run it

```bash
just prove          # compile → witness → dev keys → prove → verify
just export         # → verifier crate + instruction_data.bin
just build-program  # → deployable .so
cd ../e2e && cargo test over_9000   # verify the proof in a real Solana VM
```

Prerequisites and toolchain versions: [../RUNBOOK.md](../RUNBOOK.md).

## Experiments

- Set `power_level = 9000` and re-run `just prove` — proving fails, because no witness satisfies the
  circuit. That failure *is* the guarantee: you cannot prove a false statement.
- `just verify-snarkjs` checks the same proof in JavaScript. xark's artifacts are snarkjs-compatible,
  so one proof verifies in both a Solana program and a browser.

## Layout

```
circuit/   the Noir circuit (src/main.nr) + inputs (Prover.toml)
program/   Pinocchio verifier — one verify_instruction_data call, minimal CU
justfile   prove · export · build-program
```

Next → [02 · Age verification](../02-age-verification/)
