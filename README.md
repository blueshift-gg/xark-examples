# xark Examples

Zero-knowledge circuits written in **ordinary Rust**, proved in **TypeScript**, verified on
[Solana](https://solana.com/) with [Groth16](https://eprint.iacr.org/2016/260), all via
[**xark**](https://github.com/blueshift-gg/xark). Five worked examples, easiest to hardest, each
introducing one new idea. Every one runs end to end in its tests: the generated TypeScript client
proves, and the proof goes in a transaction to the example's program in an in-process Solana VM.

They tell one story. A prime field has no order, so a circuit compares numbers by proving them
small, bit by bit: Over 9000 teaches that **range proof**. zKYC puts range proofs to work on an age
and a net worth, beside an allow-list check and a salted commitment. Token claim brings **Merkle
trees** into the circuit. Shielded pool puts zKYC's salted commitments in a Merkle tree, with
nullifiers, so a withdrawal is unlinkable to its deposit. Shielded transfer adds range proofs back:
notes of any amount, each range-checked so value can't be minted by wrapping the field.

New to zero-knowledge? Start with the [from-zero primer](./docs/learn-zk/), the mental model
before any code.

## The idea in one paragraph

A circuit is a computation you can *prove you ran correctly* without revealing its inputs. With
xark you write it as a plain Rust function: `Private<T>` / `Public<T>` mark what's secret and what's
public, `#[input]` gives larger inputs one typed shape, and `require*` states what must hold. Your
stable `rustc` compiles it, and xark turns it into constraints. Then xark generates both ends of an
app: a **verifier crate** for your Solana program, and a **typed TypeScript client** that proves in
the browser or node, taking the circuit's own types and running the circuit crate's own Rust
helpers (a commitment, a Merkle path) so the app never re-implements them. Designing a ZK
application is mostly choosing **what's private, what's public, and what must hold between them**;
the examples below are that choice made five ways.

## The examples

| Example | Proves | New idea | Public inputs | Onchain |
|---|---|---|:-:|---|
| [01 · over-9000](./01-over-9000/) | a secret value is `> 9000` | range proofs: a field has no order, so compare by bit decomposition; the whole loop | 0 | Pinocchio |
| [02 · zKYC](./02-zkyc/) | a committed identity meets the program's KYC policy, revealing none of it | range proofs put to work (age, net worth), an allow-list that hides which country, a salted commitment, binding the signer | 5 | Pinocchio |
| [03 · token-claim](./03-token-claim/) | an address is on a list of a million, for its amount, and claims it once (a Merkle airdrop) | Merkle trees in a circuit: a leaf of address and amount, the path private | 4 | Pinocchio + SPL |
| [04 · shielded-pool](./04-shielded-pool/) | an unlinkable deposit → withdrawal (multi-denomination Tornado) | Merkle trees plus salted commitments: notes and nullifiers, a tree the program grows | 4 + 7 | Pinocchio |
| [05 · shielded-transfer](./05-shielded-transfer/) | a private payment of any amount (Zcash-style) | Merkle trees and range proofs together: range-checked amounts so the sums can't wrap; notes, keys, JoinSplit, SPL | 19 | Pinocchio + SPL |

## How it fits together

```
  Rust circuit       you write the rules: what's secret, what's public, what must hold
     │ xark build      stable rustc → wasm → R1CS
     │ xark setup      Groth16 keys (single-party here; `xark ceremony` for a multi-party one)
     ├─▶ xark export   a verifier crate, the key baked in ─▶ your Solana program
     └─▶ xark client   a typed TypeScript client
                           │ prover.prove({ ...typed inputs })
                           ▼
  your app           a 256-byte proof, sent in a transaction
                           │
                           ▼
  Solana program     verify(proof, public inputs) via the native alt_bn128 syscalls
```

From [02's test](./02-zkyc/client/zkyc.test.ts), the whole app side:

```ts
const prover = await Zkyc.prover("/circuits/zkyc/", { digest: Zkyc.digest });
const commitment = Identity.commitment(id);   // the circuit crate's Rust, run from TypeScript
const policy = Policy.default().on(new Date()).minAge(18).worthAtLeast(500_000).nationalities(["DE", "FR", "GB", "US"]);
const proof = prover.prove({ id, commitment, policy, holder: walletBytes });
// → a transaction signed by the holder, data: proof.toBytes() ‖ commitment.toBytes()
```

A program never parses the proof itself. It assembles the public inputs from its own state and the
transaction, so a proof only holds for what the program says is true, and makes one call:
`verify(&Proof::from_le_bytes(&proof), &public_inputs)`.

## Run it

```bash
npm ci          # one install for every example: the root is an npm workspace
npm run build   # every example: circuit → keys → verifier → client → program
npm test        # every example's client tests: prove, then verify onchain
```

Or one example: `cd 02-zkyc && npm run build && npm test`. Each example is its own project, with its
own `package.json` and tests, so you can copy one out and start from it (05 also takes 04's
`circuits/pool` crate). Prerequisites (stable Rust, `cargo-build-sbf`, Node, and xark):
[RUNBOOK.md](./RUNBOOK.md); `xark doctor` checks the Rust and xark side. Each example's README walks
its circuit; [01's](./01-over-9000/) also builds one from `xark init`.

## Numbers

| Circuit | Constraints | Public inputs | Prove, 1 thread | Prove, 8 workers | Onchain CU |
|---|--:|--:|--:|--:|--:|
| 01 `over_9000` | 128 | 0 | 20 ms | 9 ms | 75,916 |
| 02 `zkyc` | 1,350 | 5 | 211 ms | 70 ms | 100,204 |
| 03 `token_claim` | 5,437 | 4 | 989 ms | 195 ms | 97,846–103,846 |
| 04 `shielded_pool_deposit` | 9,647 | 4 | 1,635 ms | 403 ms | 91,139–91,724 |
| 04 `shielded_pool_withdraw` | 5,382 | 7 | 920 ms | 180 ms | 110,320–111,820 |
| 05 `shielded_transact` | 26,123 | 19 | 3,736 ms | 896 ms | 168k–174k |

Constraints from `xark inspect`. Proving in node through the generated clients; "8 workers" is
`@blueshift-gg/xark`'s worker pool. The prove times come from two machines: 01, 02, 04 and 05 in
node 24 on an Apple M4 Max, median of three; 03 in node 22 on an Apple M3 Max, median of five, so
compare its times with the others' loosely. Compute units are the whole instruction in LiteSVM, so
03, 04 and 05 include the accounts they create and the tokens or lamports they move. They vary with
the public inputs, and by 1,500 for each extra try a program takes to find a new account's bump.
05's, measured to the thousand, come close to the default 200k limit, so its client asks for 250k.
A proof is always 256 bytes.

## Testing

Each example's `client/*.test.ts` is its reference app. It proves with the generated client in
node, then sends the proof to the example's program in [LiteSVM](https://github.com/LiteSVM/litesvm),
verified through the same `alt_bn128` syscalls mainnet uses, with no validator and no network. The
tests cover the happy path and the refusals: a tampered proof, a proof for yesterday, a proof
submitted by someone else, an address not on the list, a second claim, a stranger's note, a
redirected withdrawal or fee, a double spend. When inputs can't be proved, the client says which
line of the circuit fails (`CheckFailed`, with the source line). Each circuit also has its own Rust
tests, and the programs from 02 on have host tests of their own (`npm run test:circuit`).

```
 ✓ client/over-9000.test.ts (2 tests)
 ✓ client/zkyc.test.ts (3 tests)
 ✓ client/token-claim.test.ts (2 tests)
 ✓ client/shielded-pool.test.ts (2 tests)
 ✓ client/shielded-transfer.test.ts (2 tests)
```

## How xark compares

The Solana Foundation's [`noir-examples`](https://github.com/solana-foundation/noir-examples) pair
Noir with **Sunspot** for this slot. xark takes a different developer path: circuits are plain
stable Rust (one language for circuit, gadgets, wallet logic and program, with rust-analyzer
diagnostics via `xark check`); a generated TypeScript client that proves and runs the circuit
crate's own helpers; a real multi-party setup (`xark ceremony`); Lean proofs of its constraint
rewrites; guard rails that refuse what can't be proved soundly (subset rejection with rustc spans,
a public input no constraint uses); verifier crates that record how their key was made; `xark
profile` to attribute every constraint to a source line; and a Pinocchio low-CU verify path.

## Status & safety

> Reference implementations for **learning: unaudited, not for production.** The shielded examples
> especially are educational; privacy tooling carries real legal weight depending on your
> jurisdiction. The recipes make single-party keys; use `xark ceremony` for anything real.

## Resources

- [xark](https://github.com/blueshift-gg/xark): `docs/subset.md`, `docs/integer-ops.md`, `docs/trusted-setup.md`, `docs/security.md`
- [Learn ZK, from zero](./docs/learn-zk/)

## License

MIT, see [LICENSE](./LICENSE).
