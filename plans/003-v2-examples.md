# 003 · v2 examples — UTXO pool + viewing key

Extend the difficulty ladder with a `04-` example. The pool README already names
both of these as out-of-scope-for-now, so this is the stated next rung.

## Part A — arbitrary-amount shielded transfers (UTXO / note model)

Example 03 is fixed-denomination. A UTXO model hides *amounts* and supports
arbitrary values.

- **Note** = `commitment = hash(amount, secret, nullifier)`; leaves are notes.
- **Transact** (generalizes deposit+withdraw): spend N input notes, create M
  output notes. The circuit proves:
  - membership of each spent note (N Merkle paths),
  - a nullifier per spent note,
  - **value conservation**: `Σ in == Σ out + fee`,
  - **range proofs** on every output amount (no field-overflow / negative-value
    forgery) — reuse the example-01 range mechanism.
- Program: same roots-ring + nullifier-set; one `transact` instruction inserting
  M new commitments and burning N nullifiers.

Open questions: fixed shape (2-in-2-out) vs variable; is encrypted note delivery
(recipient scanning) in scope or left to an off-chain note store; constraint/CU
budget for N paths + M range proofs.

## Part B — viewing key (selective disclosure)

Let a designated party decrypt amounts/recipients for compliance/audit without
breaking public unlinkability.

- Encrypt the note (amount, recipient) to a viewing public key; prove in-circuit
  that the ciphertext is a correct encryption of the committed note (in-circuit
  ECDH + symmetric enc — meaningfully heavier).

Open questions: which curve/scheme (embedded-curve ECDH is cheapest in Noir);
per-note vs per-pool viewing key; this is the "compliance-friendly" framing the
repo deliberately left out of 03 — confirm the product intent before building.

## Effort / risk / dependencies

L. Builds directly on 03's tree + nullifier machinery. Risk: materially more
circuit complexity and CU (range proofs + multiple paths, or in-circuit
encryption); the privacy/compliance framing carries the legal caveats 03 flags.

## Sequencing

Do Part A (UTXO) first — it's the natural graduation from 03 and doesn't need the
encryption stack. Part B (viewing key) layers on once the note model exists.

## Done when

A `04-` example proves + verifies a value-conserving multi-note transfer in the
LiteSVM suite (Part A); viewing-key selective disclosure is a follow-on (Part B).
