# 2 · Circuits & constraints

To prove something in zero knowledge, you first have to express it as a **circuit**. This is the
part that trips people up, so let's make it concrete.

## A circuit is a set of constraints, not a program

You're used to code that *runs* — it takes inputs and produces outputs step by step. A ZK circuit
is different: it's a list of **constraints** (equations) that a set of values must *satisfy*
simultaneously. The prover finds values that satisfy them; the proof attests that such values
exist and that the prover knows them.

In Noir it still *looks* like code:

```noir
fn main(power_level: u64) {
    assert(power_level > 9000);   // a constraint: this must hold
}
```

But `assert` isn't a runtime check — it's a constraint compiled into the circuit. The proof is
worthless unless every `assert` holds for the prover's secret witness.

## Everything becomes arithmetic over a finite field

Under the hood, constraints are polynomial equations over a **finite field** — integers mod a big
prime `p` (for BN254, a specific 254-bit prime). Addition and multiplication wrap around `p`.
That's the native "type" of a circuit: a *field element*. Noir's `Field` is exactly this.

Two consequences you'll feel:

- **Comparisons and bit operations aren't free.** `a > b`, `a & b`, "is this 8 bits?" all have to be
  *built* out of field arithmetic. That's what a **range proof** is — proving a value fits in N bits
  by decomposing it into bits and constraining each. ([Example 01](../../01-over-9000/) is the
  minimal case.)
- **Hashes are chosen for the field.** Poseidon is popular in ZK precisely because it's cheap in
  field arithmetic — far cheaper than SHA-256 inside a circuit. That's why our commitments and
  Merkle trees use Poseidon2.

## Witness = a satisfying assignment

The **witness** is the full set of values (private + public + all the intermediates) that satisfies
every constraint. `nargo execute` computes it from your `Prover.toml`. If your inputs *can't*
satisfy the circuit (e.g. `power_level = 9000` with `assert(power_level > 9000)`), witness
generation fails — there's nothing to prove.

## Noir → ACIR → R1CS → proof (where xark lives)

```
Noir source
   │  nargo compiles it to…
   ▼
ACIR            an "arithmetic circuit IR" — the standard, backend-agnostic circuit format
   │  xark lowers ACIR opcodes to…
   ▼
R1CS            Rank-1 Constraint System — the constraint form Groth16 proves over
   │  xark runs Groth16
   ▼
proof
```

Noir owns the frontend (source → ACIR). **xark owns the backend** (ACIR → R1CS → Groth16 →
on-chain verifier). xark supports a specific set of ACIR opcodes and *rejects* the ones it can't
prove soundly — that rejection is a safety feature, not a limitation to route around.

## Circuit cost is real

Each operation adds constraints, and constraints cost proving time. A `Poseidon2` hash is tens of
thousands; an in-circuit ECDSA verify is *millions*. Part of designing a good ZK app is keeping the
circuit small — which is why [example 03](../../03-shielded-pool/) is deliberately structured to do
all its hashing in Noir and none on-chain.

Next: [the proof system these examples use →](./03-groth16-in-one-page.md)
