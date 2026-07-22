# 04 · Shielded transfer — private payments, arbitrary amounts (Zcash-style)

Shield SPL tokens, privately pay someone an arbitrary amount — hidden value, hidden sender, hidden
recipient — then later unshield. On-chain an observer sees deposits and withdrawals but never who
paid whom, or how much. Where [03](../03-shielded-pool/) is a fixed-denomination mixer, this is a
Zcash/Sapling-style shielded pool: notes with hidden values, a key hierarchy, and shielded→shielded
transfers.

> **Production-quality educational reference, not an audited product.** It uses development setup
> keys and deliberately omits wallet, ceremony, governance, and compliance infrastructure.

## One primitive: `transact` (JoinSplit)

A single circuit and instruction cover deposit, private transfer, and withdraw. The trick is Zcash's
JoinSplit: spend 2 input notes, create 2 output notes, and balance the difference against two
*public* value slots — `vpub_in` (value entering the shield) and `vpub_out` (value leaving it):

```
shielded_in(2 notes) + vpub_in  ==  shielded_out(2 notes) + vpub_out + fee
```

- **Deposit** — `vpub_in > 0` (SPL enters the vault), inputs are dummies, outputs are your new
  shielded notes.
- **Transfer** — `vpub_in = vpub_out = 0`, spend 2 notes → create 2 notes (recipient + change).
  **No tokens move on-chain**; only commitments and nullifiers change.
- **Withdraw** — `vpub_out > 0` (SPL leaves the vault to a token account).

Slots you don't need are filled with **dummy notes** (value 0, membership skipped) — the standard way
to keep a fixed 2-in / 2-out shape whatever the operation.

## What the circuit proves

- **Notes.** A note is `cm = H(value, asset, owner_addr, rho, rseed)` (the length-tagged Poseidon2
  sponge plus an explicit domain constant). `rho` and `rseed` are randomness that keep
  otherwise-identical notes distinct and unlinkable.
- **Key hierarchy (Sapling-lite).** `sk → nk` (nullifier key) and `ivk` (incoming viewing key); an
  address is `addr = H(ivk, d)`. Only the holder of `sk` can derive the keys to spend a note sent to
  their address — spend authority and view authority are separate.
- **Nullifier.** `nf = H(nk, cm, position)`, bound to the owner, the note, *and* its position in the
  tree. It is deterministic, so a note has exactly one nullifier (revealed once → no double-spend),
  yet unlinkable to the commitment without `nk`.
- **The rest, in one shot.** Membership of each real input in a remembered root; value conservation
  (the balance identity above); range proofs on output values (so you can't forge value by
  overflowing the field); and — 03's trick again — that the two output commitments are correctly
  appended (`old_root → new_root` via a private frontier), so the chain never hashes. Because the
  program always appends notes two at a time, `insert_index` stays even and the pair folds in as one
  level-1 node (`hash2(cm0, cm1)`), halving the membership work.

Eighteen public inputs: `root, asset, nf[2], cm_out[2], old_root, new_root, insert_index, vpub_in,
vpub_out, fee, recipient_hi, recipient_lo, memo0_hash_hi, memo0_hash_lo, memo1_hash_hi,
memo1_hash_lo`. The circuit is ordinary Rust importing 03's Merkle gadget crate; `xark profile`
attributes every constraint to a source line.

## Delivery: encrypted memos, off-circuit

How does the recipient learn the `rho`/`rseed`/value of the note you made for them? The sender
attaches an encrypted memo envelope containing the ephemeral key and ciphertext, emitted here as a
`MemoEvent`, and the recipient trial-decrypts every envelope. SHA-256 hashes of both envelopes are
bound into the proof as four 128-bit limbs. The program recomputes those hashes, so a submitter can
relay the transaction but cannot replace delivery data or redirect `recipient_token`. Encryption and
decryption remain off-circuit, so no expensive in-circuit AEAD is needed.

## SPL

Native SPL from the start: one pool and one token **vault** (a PDA-owned token account) per mint. The
circuit's `asset` field is derived from the mint, so notes are per-token. Deposits `transfer` tokens
into the vault; withdrawals `transfer` out, signed by the pool PDA.

## Run it

```bash
just all
# xark build + setup (dev keys) → build the deposit/transfer/withdraw proof chain →
# export verifier + build program → run the in-VM e2e (shielded_transfer_flow)
```

The e2e ([`../e2e/tests/transfer.rs`](../e2e/tests/transfer.rs)) runs the whole story in LiteSVM:
Alice shields 150 → privately pays Bob 100 with 50 change (no tokens move) → Bob withdraws 100; the
vault ends at 50, Alice's change still shielded. The `transfer-witness` bin in [`../e2e`](../e2e/)
is the reference wallet: it replays the evolving tree natively (same KAT-pinned Poseidon2 as the
circuit) and writes each transaction's typed `xark prove --inputs` document. Its names are grouped
under `witness.*` and `statement.*`; `xark inspect` shows the exact flattened order consumed by the
verifier.

## Anatomy

```
circuits/transact/   the JoinSplit circuit (notes, keys, nullifiers, balance, range,
                     membership, pair insertion) — imports 03's circuits/merkle gadget
program/             Anchor + SPL: verify, burn nullifiers, advance tree, move SPL for
                     vpub in/out, emit memo events
../e2e               transfer-witness (wallet-side chain replay) + the LiteSVM e2e
```

## Deployment boundaries

The protocol path is complete, but a deployed product still needs the surrounding operational
system:

- **Fixed 2-in / 2-out, one asset per transaction** (`asset` is public per tx).
- **Dev trusted setup** — forgeable; a real deployment needs `xark ceremony`.
- **Fee routing is not implemented** — `fee` is bound in-circuit (the slot a relayer flow needs) but
  the program requires it to be 0 rather than silently ignoring it.
- **Memo encryption** is a wallet concern. The proof binds the complete encrypted envelope hash, but
  the example does not implement x25519/ChaCha encryption, key storage, or trial-decryption UX.
- **Single-writer tree tip.** Each proof binds the current append root and index. A relayer must
  sequence transactions, and clients must reprove when another transaction advances the tip first.
  Higher throughput needs a sequencer/batcher or a different append architecture.
- **Protocol review and operations.** The simplified `nk`/`ivk` derivation is not the full Sapling
  specification; a deployment also needs an audit, ceremony, upgrade policy, monitoring, and legal
  review.

## Reference

### Instructions

**`initialize()`** — create the pool and token `vault` for one SPL mint. Accounts: `pool` (init,
PDA), `vault` (init, token account PDA, authority = `pool`), `mint`, `authority` (signer, payer),
`token_program`, `system_program`, `rent`.

**`transact(proof, root, nf: [[u8;32];2], cm_out: [[u8;32];2], new_root, vpub_in: u64, vpub_out: u64,
fee: u64, memo0: Vec<u8>, memo1: Vec<u8>)`** — one JoinSplit: verify the proof, burn `nf[0..2]`,
append `cm_out[0..2]` (advancing `old_root → new_root`), move SPL for the public value slots, and emit
the two memos. Accounts: `pool` (mut), `vault` (mut), `nullifier0`/`nullifier1` (init, PDA), `user`
(signer, payer), `user_token` (mut, source for `vpub_in`), `recipient_token` (mut, dest for
`vpub_out`), `token_program`, `system_program`. Public inputs:
`root, asset, nf0, nf1, cm0, cm1, old_root, new_root, index, vpub_in, vpub_out, fee`, both limbs of
`recipient_token`, and both limbs of each memo envelope's SHA-256 hash.

### PDAs & state

| PDA | Seeds |
|---|---|
| `pool` | `["pool", mint]` |
| `vault` | `["vault", mint]` — token account, authority = `pool` |
| `nullifier0` / `nullifier1` | `["nullifier", mint, nf[i]]` |

`Pool` state: `mint: Pubkey`, `next_index: u32`, `current_root_index: u32`, `roots: [[u8;32]; 16]`,
`bump: u8`. `asset` (the circuit's per-token tag) is the low 16 bytes of the mint pubkey.

### Events & errors

`MemoEvent { leaf_index: u32, commitment: [u8;32], encrypted_memo: Vec<u8> }` — one per output note;
the envelope contains the ephemeral key and ciphertext the recipient trial-decrypts.

| Error | Message |
|---|---|
| `BadProofLen` | proof must be 256 bytes |
| `InvalidProof` | invalid proof |
| `UnknownRoot` | unknown or stale merkle root |
| `TreeFull` | tree is full |
| `FeeNotSupported` | fee routing is not implemented; pass fee = 0 |

Back to the [index](../README.md).
