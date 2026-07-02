# 04 · Shielded transfer — arbitrary amounts, private payments (Zcash-style)

> Goal: shield SPL tokens, **privately pay someone an arbitrary amount** (hidden
> value, hidden sender, hidden recipient), and later unshield — the on-chain
> graph shows deposits and withdrawals but not who paid whom, or how much.

Where [03](../03-shielded-pool/) is a fixed-denomination mixer, 04 is a
**Zcash/Sapling-style shielded pool**: notes with hidden values, a key
hierarchy, and shielded→shielded transfers. It's the real thing.

> ⚠️ **Reference implementation. Unaudited. Educational.** Do not hold value in
> this. See "Known gaps" below — some are load-bearing.

## The one primitive: `transact` (JoinSplit)

A single circuit + instruction does **deposit, private transfer, and withdraw**
via public value slots — exactly the Zcash JoinSplit idea:

```
  shielded_in(2 notes) + vpub_in  ==  shielded_out(2 notes) + vpub_out + fee
```

- **Deposit** — `vpub_in > 0` (SPL enters the vault), inputs are dummy, outputs
  are your new shielded notes.
- **Transfer** — `vpub_in = vpub_out = 0`, spend 2 notes → create 2 notes (e.g.
  recipient + change). **No tokens move on-chain.**
- **Withdraw** — `vpub_out > 0` (SPL leaves the vault to a token account).

Inputs you don't need are **dummies** (value 0, membership skipped) — the
standard way to fill fixed 2-in/2-out slots.

## What the circuit proves

- **Notes:** `cm = H(value, asset, owner_addr, rho, rseed)` (Poseidon2 sponge).
- **Key hierarchy (Sapling-lite):** `sk → nk` (nullifier key), `ivk` (viewing
  key), `addr = H(ivk, d)`. Only the owner can spend.
- **Nullifier:** `nf = H(nk, cm, position)` — bound to owner, note, and tree
  position; revealed once to prevent double-spends.
- **Membership** of each real input in a remembered root; **value conservation**;
  **range proofs** on outputs (no field-overflow value forgery); and the
  **output commitments are correctly appended** to the tree (`old_root → new_root`
  via a private frontier) so the chain never hashes (03's trick, ×2 leaves).

12 public inputs; all opcodes xark-supported.

## Encrypted memos (delivery)

Note delivery is **off-circuit**, exactly as Zcash does it: the sender attaches
an encrypted memo (`epk`, ciphertext) — emitted here as a `MemoEvent` — and the
recipient trial-decrypts to find their note. Funds security (commitments +
nullifiers) is fully in-circuit; the memo is just the delivery channel, so no
expensive in-circuit AEAD is needed.

## SPL

Native SPL from day one: one pool + token **vault** per mint (PDA-owned token
account). `asset` (a circuit field) is derived from the mint, so notes are
per-token. Deposits `transfer` into the vault; withdrawals `transfer` out, signed
by the pool PDA.

## Run it

```bash
just all
# setup (dev keys) → generate the deposit/transfer/withdraw proof chain →
# export verifier + build program → run the in-VM e2e:
#   shielded_transfer_flow ... ok
```

The e2e ([`../e2e/tests/transfer.rs`](../e2e/tests/transfer.rs)) runs the whole
story in LiteSVM: Alice shields 150 → privately pays Bob 100 (50 change, no
tokens move) → Bob withdraws 100; the vault ends at 50 (Alice's change stays
shielded). The Rust test doubles as the reference wallet (key/note/witness
construction); `scripts/gen_chain.py` builds the witnesses.

## Anatomy

```
circuits/transact/   the JoinSplit circuit (notes, keys, nullifiers, balance,
                     range, membership, 2-leaf insertion) + the chain simulator
program/             Anchor + SPL: verify, burn nullifiers, advance tree, move
                     SPL for vpub in/out, emit memo events
scripts/gen_chain.py builds the 3 witnesses/proofs for the e2e
```

## Known gaps (read before trusting it)

These are deliberate scope cuts for a teachable reference — and the honest list
of what a production version must add:

- **The withdrawal recipient is not bound into the proof.** `vpub_out` pays
  whatever `recipient_token` the transaction passes, so a front-runner could
  redirect a withdrawal. Fix: add the recipient (owner) as a public input the
  circuit binds, like [03](../03-shielded-pool/) does. **Top hardening item.**
- **Fixed 2-in/2-out**, single asset per tx (asset is public per tx).
- **Dev trusted setup** (`--insecure-dev-mode`) — forgeable; use `xark ceremony`.
- **Memo encryption** is left to a standard off-circuit AEAD (x25519 + ChaCha);
  the epk-binding that Zcash adds in-circuit for robustness is a future layer
  (the embedded-curve op is validated as available).
- Unaudited; `nk`/`ivk` derivation is a simplified Sapling, not the full spec.

Back to the [gallery](../README.md).
