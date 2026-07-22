# 2 - Circuits & constraints

To prove something in zero knowledge, first express it as a **circuit**. The code looks familiar,
but its meaning is stricter than an ordinary program.

## A circuit is a set of constraints

Ordinary code runs instructions and returns a result. A ZK circuit describes equations that a set
of values must satisfy simultaneously. The prover supplies a satisfying assignment, called the
**witness**, and produces a proof that the constraints held.

With xark, the circuit is ordinary Rust:

```rust
use xark::prelude::*;

#[circuit]
pub fn over_9000(power_level: Private<u64>) {
    require(power_level > 9000u64);
}
```

`Private<u64>` marks a secret, range-constrained witness input. `Public<Field>` marks a field value
the verifier receives. The requirement becomes constraints; it is not a check the prover can bypass
at runtime. Larger inputs can derive `CircuitInput` and remain normal typed Rust structures; xark
flattens their leaves into named inputs deterministically.

## Everything becomes finite-field arithmetic

Constraints are polynomial equations over the BN254 scalar field. Addition and multiplication wrap
modulo a large prime. That has two practical consequences:

- **Ordering and bit operations need explicit bounds.** The `u64` in `> 9000u64` makes xark
  decompose the value into 64 constrained bits before comparing it.
- **Circuit-friendly hashes matter.** Poseidon2 is much cheaper in field arithmetic than SHA-256,
  so the shielded examples use it for commitments, nullifiers, and Merkle nodes.

## Witness = a satisfying assignment

The witness includes private inputs, public inputs, and every intermediate value. `xark prove`
solves those intermediates from an input file. If the inputs cannot satisfy the circuit, such as
`power_level = 9000`, proof generation fails because no valid witness exists.

## Rust -> MIR -> xark-IR -> R1CS -> proof

```
Rust circuit
   | rustc type-checks it; xark extracts MIR
   v
xark-IR          a small, auditable circuit intermediate representation
   | xark lowers field operations and requirements
   v
R1CS             Rank-1 Constraint System, the form Groth16 proves
   | xark setup + prove
   v
Groth16 proof
```

xark intentionally supports a constrained Rust subset. `xark check` reports unsupported behavior
with rustc source spans instead of guessing at semantics. Rejection is part of the soundness model.

## Circuit cost is real

Every range check and hash adds constraints, which affects proving time and memory. Use
`xark inspect` for totals and `xark profile` to attribute constraints to Rust source lines. The later
examples keep tree hashing inside the circuit to keep the Solana program and wallet on one exact
Poseidon2 definition, at the cost of state-dependent proof generation.

Next: [the proof system these examples use ->](./03-groth16-in-one-page.md)
