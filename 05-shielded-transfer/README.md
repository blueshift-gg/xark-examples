# 05 · Shielded transfer: private payments, any amount (Zcash-style)

Shield SPL tokens, privately pay someone any amount (hidden value, hidden sender, hidden recipient),
then later unshield. Onchain an observer sees deposits and withdrawals but never who paid whom, or
how much. Where [04](../04-shielded-pool/) is a fixed-denomination mixer, this is a
Zcash/Sapling-style shielded pool: notes with hidden values, a key hierarchy, and shielded→shielded
transfers.

> **Educational reference, not an audited product.** It uses development setup keys and
> deliberately omits wallet, ceremony, governance, and compliance infrastructure.

## One primitive: `transact` (JoinSplit)

A single circuit and instruction cover deposit, private transfer, and withdrawal. The trick is
Zcash's JoinSplit: spend 2 notes, create 2 notes, and balance the difference against two *public*
value slots, `vpub_in` (value entering the pool) and `vpub_out` (value leaving it):

```
spent(2 notes) + vpub_in  ==  created(2 notes) + vpub_out + fee
```

- **Deposit**: `vpub_in > 0` (SPL enters the vault), the spends are dummies, the outputs are your
  new notes.
- **Transfer**: `vpub_in = vpub_out = 0`; spend notes, create notes (the payee's and your change).
  **No tokens move onchain**; only commitments and nullifiers change.
- **Withdraw**: `vpub_out > 0` (SPL leaves the vault to a token account).

Slots you don't need take **dummy** spends (`real: false`, value 0, no membership check), the
standard way to keep a fixed 2-in / 2-out shape whatever the operation.

## The circuit's types

The circuit's inputs are its domain, as Rust types the TypeScript client gets as they are:

```rust
#[input] pub struct Key { pub secret: Field }
#[input] pub struct Note { pub value: u64, pub owner: Field, pub rho: Field, pub rseed: Field }
#[input] pub struct Spend { pub note: Note, pub diversifier: Field, pub path: Path, pub real: bool }
#[input] pub struct Flow { pub vpub_in: u64, pub vpub_out: u64, pub fee: u64, pub recipient: Hash, pub relayer: Hash }

/// What the program checks a transaction against: the public inputs, in this order.
#[input]
pub struct Transaction {
    pub root: Field, pub asset: Hash, pub nullifiers: [Field; 2], pub commitments: [Field; 2],
    pub old_root: Field, pub new_root: Field, pub index: u32, pub flow: Flow, pub memos: [Hash; 2],
}

#[circuit]
pub fn shielded_transact(key: Private<Key>, spends: Private<[Spend; 2]>, outputs: Private<[Note; 2]>,
                         tip: Private<Path>, tx: Public<Transaction>)
```

- **Keys (Sapling-lite).** `Key::nullifier_key` and `Key::viewing_key` derive from the secret;
  `Key::address(diversifier)` from the viewing key, as many unlinkable addresses as the holder
  likes. Spending needs the secret; recognizing your addresses needs only the viewing key.
- **Notes.** `Note::commitment(asset)` hashes every field with the pool's mint, so notes are
  per-token. `rho` and `rseed` keep otherwise-identical notes distinct and hidden.
- **Nullifiers.** `Note::nullifier(key, asset, position)` binds the spender's key, the note *and*
  its position in the tree: one nullifier per note (revealed once, so no double spend), unlinkable
  to the commitment without the key.

Each is `#[input] impl` Rust that the circuit uses and the client calls, so the wallet computes
commitments, addresses and nullifiers with the code the proof checks: `Key.address(key, d)`,
`Note.commitment(note, mint)`, `Note.nullifier(note, key, mint, position)`. And one call builds the
whole public statement from them: `Transaction.of(key, spends, outputs, tip, mint, flow, memos)`.

## What the circuit proves

In one proof: each real spend is in the tree at a remembered `root`, owned by the spender
(`note.owner == key.address(diversifier)`), and burns the nullifier revealed; each output is the
commitment revealed; value is conserved; and, 04's trick again, the two outputs are appended
(`old_root → new_root`) so the chain never hashes. Every value is a `u64` input, range-checked on
entry, so no sum can wrap the field. The program always appends notes in pairs at an even slot, so
the second output's path is the first's with the first beside it: two `xark-merkle` roots, before
and after, prove the whole append.

The public inputs are one `Transaction`: 19 field elements, `root, asset (hi, lo), nullifiers[2],
commitments[2], old_root, new_root, index, flow, recipient (hi, lo), relayer (hi, lo), memos[2]
(hi, lo each)`. `flow` is `Flow`'s three `u64`s packed into one element, as xark packs a struct's
narrow fields: `vpub_in + vpub_out·2⁶⁴ + fee·2¹²⁸`. Addresses (the mint, the token accounts) and memo hashes are all
32 bytes, which the circuit takes as xark's `Hash`. The circuit imports 04's `pool` crate for `Path`;
`xark profile` attributes every constraint to a source line.

## Delivery: encrypted memos, off-circuit

How does the payee learn the value, `rho` and `rseed` of the note you made for them? The sender
attaches an encrypted memo per note, logged as a memo event, and owners trial-decrypt every memo.
Each memo's SHA-256 is a public input, as xark's `Hash` (its 32 bytes, in two halves), which the
program recomputes. A relayer can submit the transaction but can't swap a memo, or redirect the
withdrawal or its own fee. Encryption stays off-circuit, so there's no expensive in-circuit AEAD.

## Relayers

The spender shouldn't sign their own private transfer: their wallet would pay the fee and the rent
for the nullifier accounts, and so appear onchain beside it. So anyone can submit a transaction,
and take `fee` of the pool's token for it. `fee` and the relayer's token account are public inputs
(`Flow`), so the proof pays only the relayer it was made for, and the fee comes out of the notes
spent: `spent + vpub_in == created + vpub_out + fee`. In the [test](./client/shielded-transfer.test.ts),
Bob holds no SOL at all, and a relayer submits both Alice's payment and his withdrawal; someone else
submitting his withdrawal, redirected to their account, fails.

## SPL

One pool and one token **vault** (a PDA-owned token account) per mint. Deposits transfer tokens into
the vault; withdrawals and fees transfer out, signed by the pool PDA. A transaction names only the
token accounts it uses: a deposit its source (`user_token`), a withdrawal its destination
(`recipient_token`), a fee the relayer's (`relayer_token`).

A transact uses 168k to 174k compute units (measured in the test: a deposit 173k, a private
transfer 170k, a withdrawal 168k; it varies with the public inputs), almost all of it the Groth16
check. That's close to the default limit of 200k, so the client asks for 250k with a compute budget
instruction.

## The program

[`program/`](./program/src/lib.rs) is one Pinocchio crate, no framework, so every account check is
written out, in `initialize_accounts` and `transact_accounts`, and runs before anything changes:
signers and writable flags; the token and system program ids; the pool owned by the program, of a
pool's size, at its PDA; the vault at its PDA, the pool's token account of the pool's mint; each
nullifier at its canonical PDA and not yet spent; each token account given the token program's,
initialized, and of the pool's mint. `initialize` refuses a pool or vault that already exists, and
pays each new account's rent-exempt minimum. The program's host tests (`cargo test --release` in
`program/`) refuse each of these on fake accounts, and check that the demo proof verifies against
the public inputs the program assembles.

## Run it

```bash
just build          # circuit → key → verifier crate → client → .so
just test-circuit   # deposit, pay privately, and what's refused; then the program's host tests
cd .. && npx vitest run 04   # prove with the client, transact onchain (LiteSVM)
```

The [test](./client/shielded-transfer.test.ts) is the reference wallet: Alice shields 150, then
pays Bob 100 privately with 45 change and 5 to a relayer (only the fee moves onchain), and Bob,
who holds no SOL, withdraws 95 through the relayer for another 5. The vault ends at 45 (Alice's
change, still shielded); Bob's withdrawal, redirected by someone else to their account or their fee,
fails; a second withdrawal of Bob's note is refused; and Mallory can't spend Alice's note: the
client names the failing line, `key.address(spend.diversifier)`. The wallet keeps every note's
`Path` current as outputs land, with 04's `Path.update`.

It proves across 8 worker threads: [`pooled`](./client/harness.ts) hands the client `@blueshift-gg/xark` with
`prove` routed through its worker pool, as the prover's `wasm` option. On an M4 Max that's about
0.9 s per proof instead of 3.7 s, with the same proofs and the same errors.

## Anatomy

```
circuits/transact/   the JoinSplit circuit: keys, notes, nullifiers, balance, membership, the
                     pair append; imports 04's circuits/pool
client/              the generated client, and the wallet test
program/             Pinocchio + SPL: verify, burn nullifiers, advance the tree, move SPL for
                     vpub_in/out, log memo events
```

## Deployment boundaries

The protocol path is complete, but a deployed product still needs the surrounding system:

- **Fixed 2-in / 2-out, one asset per transaction** (`asset` is public per transaction).
- **Dev trusted setup**: forgeable; a real deployment needs `xark ceremony`.
- **Memo encryption** is a wallet concern. The proof binds each memo's hash, but the example
  doesn't implement x25519/ChaCha encryption, key storage, or trial-decryption UX.
- **Single-writer tree tip.** Each proof binds the current root and append index. A relayer must
  sequence transactions, and clients must prove again when another transaction lands first.
- **Protocol review and operations.** The simplified key derivation is not the full Sapling
  specification; a deployment also needs an audit, a ceremony, an upgrade policy, monitoring, and
  legal review.

## Reference

### Instructions

Instruction data starts with a one-byte tag; the arguments follow, fixed-size ones as their bytes
and integers little-endian.

**`0` initialize** (no arguments): create the pool and token `vault` for one SPL mint. Accounts:
`pool` (writable, PDA, not yet created), `vault` (writable, PDA, not yet created: becomes a token
account of `mint`, authority = `pool`), `mint` (an initialized SPL mint), `authority` (writable
signer, pays both accounts' rent), `token_program`, `system_program`.

**`1` transact**: `proof` (256 B) ‖ `root` ‖ `nf[0]` ‖ `nf[1]` ‖ `cm_out[0]` ‖ `cm_out[1]` ‖
`new_root` (32 B each) ‖ `vpub_in` ‖ `vpub_out` ‖ `fee` (u64 each) ‖ `memo0` ‖ `memo1` (each a u32
length, then its bytes). One JoinSplit: verify the proof, burn `nf[0..2]`, append `cm_out[0..2]`
(`old_root → new_root`), move SPL for the public value slots, and log the two memos. Accounts:
`pool` (writable), `vault` (writable), `nullifier0`/`nullifier1` (writable, PDAs, created here),
`user` (writable signer, pays the nullifiers' rent), `user_token` (optional, writable: the source
when `vpub_in > 0`), `recipient_token` (optional, writable: the destination when `vpub_out > 0`),
`relayer_token` (optional, writable: the destination when `fee > 0`), `token_program`,
`system_program`. A left-out optional account is passed as the program's address, and the proof
then binds the program's address in its place. Any token account given is of the pool's mint.

### PDAs & state

| PDA | Seeds | Owner, data |
|---|---|---|
| `pool` | `["pool", mint]` | the program, `Pool` (554 B) |
| `vault` | `["vault", mint]` | the token program: a token account, authority = `pool` |
| `nullifier0` / `nullifier1` | `["nullifier", mint, nf[i]]` | the program, no data: it exists once spent |

`Pool`, byte arrays with no padding: `mint` (32 B) at 0, `next_index` (u32) at 32,
`current_root_index` (u32) at 36, `roots` (16 × 32 B, a ring) at 40, `bump` at 552, `vault_bump`
at 553. The bumps are the canonical ones `initialize` found.

### Events & errors

One memo event per output note, through `sol_log_data` (`Program data:` in the logs, each field
base64): `leaf_index` (u32), `commitment` (32 B), the encrypted memo.

Account checks fail with the runtime's errors (`MissingRequiredSignature`, `Immutable`,
`IncorrectProgramId`, `InvalidAccountOwner`, `InvalidAccountData`, `InvalidSeeds`,
`AccountAlreadyInitialized`, `UninitializedAccount`, `IncorrectAuthority`); the program's own are
`Custom(code)`, and each logs its name:

| Code | Error | When |
|---|---|---|
| 0 | `InvalidProof` | the proof doesn't verify against the assembled public inputs |
| 1 | `UnknownRoot` | `root` isn't one of the last 16 |
| 2 | `TreeFull` | no room for two more leaves |
| 3 | `NoSource` | a deposit (`vpub_in > 0`) without `user_token` |
| 4 | `NoRecipient` | a withdrawal (`vpub_out > 0`) without `recipient_token` |
| 5 | `NoRelayer` | a fee (`fee > 0`) without `relayer_token` |
| 6 | `NullifierSpent` | a nullifier's account exists: the note was spent |
| 7 | `WrongMint` | a token account of another mint than the pool's |

Back to the [index](../README.md).
