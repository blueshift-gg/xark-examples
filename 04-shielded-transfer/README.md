# 04 · Shielded transfer — private payments, arbitrary amounts (Zcash-style)

Shield SPL tokens, privately pay someone an arbitrary amount — hidden value, hidden sender, hidden
recipient — then later unshield. On-chain an observer sees deposits and withdrawals but never who
paid whom, or how much. Where [03](../03-shielded-pool/) is a fixed-denomination mixer, this is a
Zcash/Sapling-style shielded pool: notes with hidden values, a key hierarchy, and shielded→shielded
transfers.

> **Reference implementation. Unaudited. Educational.** Do not hold value in this. See "Known gaps" —
> some are load-bearing.

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

- **Notes.** A note is `cm = H(value, asset, owner_addr, rho, rseed)` (a Poseidon2 sponge). `rho` and
  `rseed` are randomness that keep otherwise-identical notes distinct and unlinkable.
- **Key hierarchy (Sapling-lite).** `sk → nk` (nullifier key) and `ivk` (incoming viewing key); an
  address is `addr = H(ivk, d)`. Only the holder of `sk` can derive the keys to spend a note sent to
  their address — spend authority and view authority are separate.
- **Nullifier.** `nf = H(nk, cm, position)`, bound to the owner, the note, *and* its position in the
  tree. It is deterministic, so a note has exactly one nullifier (revealed once → no double-spend),
  yet unlinkable to the commitment without `nk`.
- **The rest, in one shot.** Membership of each real input in a remembered root; value conservation
  (the balance identity above); range proofs on output values (so you can't forge value by
  overflowing the field); and — 03's trick again, twice — that the two output commitments are
  correctly appended (`old_root → new_root` via a private frontier), so the chain never hashes.

Twelve public inputs: `root, asset, nf[2], cm_out[2], old_root, new_root, insert_index, vpub_in,
vpub_out, fee`.

## Delivery: encrypted memos, off-circuit

How does the recipient learn the `rho`/`rseed`/value of the note you made for them? The sender
attaches an encrypted memo — `(epk, ciphertext)`, emitted here as a `MemoEvent` — and the recipient
trial-decrypts every memo with their viewing key to find the ones addressed to them. This is exactly
Zcash's design, and it's off-circuit on purpose: **funds security** (commitments + nullifiers) is
fully enforced in the proof, while **delivery** is only a channel, so no expensive in-circuit AEAD is
needed.

## SPL

Native SPL from the start: one pool and one token **vault** (a PDA-owned token account) per mint. The
circuit's `asset` field is derived from the mint, so notes are per-token. Deposits `transfer` tokens
into the vault; withdrawals `transfer` out, signed by the pool PDA.

## Run it

```bash
just all
# setup (dev keys) → build the deposit/transfer/withdraw proof chain →
# export verifier + build program → run the in-VM e2e (shielded_transfer_flow)
```

The e2e ([`../e2e/tests/transfer.rs`](../e2e/tests/transfer.rs)) runs the whole story in LiteSVM:
Alice shields 150 → privately pays Bob 100 with 50 change (no tokens move) → Bob withdraws 100; the
vault ends at 50, Alice's change still shielded. That Rust test doubles as the reference wallet
(key/note/witness construction), and `scripts/gen_chain.py` builds the three witnesses by replaying
an evolving tree.

## Anatomy

```
circuits/transact/   the JoinSplit circuit (notes, keys, nullifiers, balance, range,
                     membership, 2-leaf insertion); tests + chain simulator in src/tests.nr
program/             Anchor + SPL: verify, burn nullifiers, advance tree, move SPL for
                     vpub in/out, emit memo events
scripts/gen_chain.py builds the 3 witnesses/proofs for the e2e
```

## Known gaps

Deliberate scope cuts for a teachable reference — and the honest list of what a production version
must add:

- **The withdrawal recipient is not bound into the proof.** `vpub_out` pays whichever
  `recipient_token` account the transaction names, so a front-runner could redirect a withdrawal. The
  fix is to add the recipient as a public input the circuit binds, exactly as
  [03](../03-shielded-pool/) does. **Top hardening item.**
- **Fixed 2-in / 2-out, one asset per transaction** (`asset` is public per tx).
- **Dev trusted setup** (`--insecure-dev-mode`) — forgeable; a real deployment needs `xark ceremony`.
- **Memo encryption** is left to a standard off-circuit AEAD (x25519 + ChaCha); the in-circuit `epk`
  binding Zcash adds for robustness is a later layer (the embedded-curve op is validated as
  available).
- Unaudited, and `nk`/`ivk` derivation is a simplified Sapling, not the full spec.

Back to the [index](../README.md).
