# 001 · Benchmark vs Sunspot

## Goal

Replace the qualitative "How xark compares" claims in the README (and the
unmeasured "~a few hundred k CU" line in `docs/learn-zk/03`) with **measured
numbers** — for xark, and side-by-side vs Sunspot where feasible.

## Metrics

Per circuit (over-9000, age-verification, deposit, withdraw):

- **On-chain verify CU** — the headline number.
- **Proof size** (bytes) — expect 256 for all (Groth16/BN254).
- **Constraint count** — from `xark inspect` ("Estimated R1CS constraints").
- **Prover time** — wall-clock of `xark prove` (note: dev-mode setup).
- **Program size** (`.so` bytes) and **verify CU: Pinocchio vs Anchor** (01/02 vs 03).

## Approach

- Extend the `e2e/` LiteSVM harness: `send_transaction` returns
  `TransactionMetadata { compute_units_consumed, .. }` — assert-and-record it
  instead of discarding. Emit a table (stdout or a committed CSV — data, not prose).
- xark side is cheap: the harness already loads each program and submits a real
  proof; just read the CU out.
- **Sunspot side is the hard part.** Install Sunspot, run the same Noir circuits
  through it, deploy its verifier, and meter the same CU. This is where most of
  the effort (and most of the risk) sits.

## Open questions (decide first)

- Can Sunspot consume the *same* Noir circuits unchanged, or do they need
  porting? If porting, the comparison is less apples-to-apples — document it.
- Neutrality: publish methodology + raw numbers so it reads as evidence, not
  advocacy. Who owns keeping it current as both backends evolve?
- Where do the numbers live so they don't rot — generated on demand vs committed
  snapshot with a date.

## Effort / risk

L. Risk: a comparison that looks like advocacy if not rigorous; numbers drift.

## Done when

A reproducible command prints verify-CU / proof-size / constraint-count for all
xark circuits, and (stretch) a fair side-by-side vs Sunspot with documented
methodology.
