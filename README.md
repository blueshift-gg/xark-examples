# xark Examples

Zero-knowledge circuits written in **ordinary Rust**, proved in **TypeScript**, verified on
[Solana](https://solana.com/) with [Groth16](https://eprint.iacr.org/2016/260), all via
[**xark**](https://github.com/blueshift-gg/xark). Four worked examples, easiest to hardest, each
introducing one new idea. Every one runs end to end in its tests: the generated TypeScript client
proves, and the proof goes in a transaction to the example's program in an in-process Solana VM.

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
the examples below are that choice made four ways.

## The examples

| Example | Proves | New idea | Public inputs | On-chain |
|---|---|---|:-:|---|
| [01 · over-9000](./01-over-9000/) | a secret value is `> 9000` | range proofs, the whole loop | 0 | Pinocchio |
| [02 · age-verification](./02-age-verification/) | 18 or older today, without a birth date | commitments, typed policies, binding the signer | 4 | Pinocchio |
| [03 · shielded-pool](./03-shielded-pool/) | an unlinkable deposit → withdrawal (multi-denomination Tornado) | Merkle paths, nullifiers, a wallet in Rust | 4 + 7 | Anchor |
| [04 · shielded-transfer](./04-shielded-transfer/) | a private payment of any amount (Zcash-style) | notes, keys, JoinSplit, SPL, relayers | 21 | Anchor + SPL |

## How it fits together

```
  Rust circuit       you write the rules: what's secret, what's public, what must hold
     │ xark build      stable rustc → wasm → R1CS
     │ xark setup      Groth16 keys (dev keys here; `xark ceremony` for real ones)
     ├─▶ xark export   a verifier crate, the key baked in ─▶ your Solana program
     └─▶ xark client   a typed TypeScript client
                           │ prover.prove({ ...typed inputs })
                           ▼
  your app           a 256-byte proof, sent in a transaction
                           │
                           ▼
  Solana program     verify(proof, public inputs) via the native alt_bn128 syscalls
```

From [02's test](./02-age-verification/client/age-verification.test.ts), the whole app side:

```ts
const prover = await AgeVerification.prover("/circuits/age_verification/", { digest: AgeVerification.digest });
const commitment = Identity.commitment(id);   // the circuit crate's Rust, run from TypeScript
const proof = prover.prove({ id, commitment, policy: Policy.adultOn(new Date()), holder: walletBytes });
// → a transaction signed by the holder, data: proof.toBytes() ‖ commitment.toBytes()
```

A program never parses the proof itself. It assembles the public inputs from its own state and the
transaction, so a proof only holds for what the program says is true, and makes one call:
`verify(&Proof::from_le_bytes(&proof), &public_inputs)`.

## Run it

```bash
npm install
just test   # every example: circuit → keys → verifier → client → program, then the client tests
```

Or one example: `cd 02-age-verification && just test`. Each example is its own project, with its
own `package.json` and tests, so you can copy one out and start from it (04 also takes 03's
`circuits/pool` crate). Prerequisites (stable Rust, `cargo-build-sbf`, Node, `just`, and xark):
[RUNBOOK.md](./RUNBOOK.md); `xark doctor` checks the Rust and xark side. Each example's README walks
its circuit; [01's](./01-over-9000/) also builds one from `xark init`.

## Numbers

| Circuit | Constraints | Public inputs | Prove, 1 thread | Prove, 8 workers | On-chain CU |
|---|--:|--:|--:|--:|--:|
| 01 `over_9000` | 128 | 0 | 20 ms | 9 ms | 73,912 |
| 02 `age_verification` | 325 | 4 | 72 ms | 24 ms | 95,175 |
| 03 `shielded_pool_deposit` | 9,688 | 4 | 1,635 ms | 403 ms | 101,041 |
| 03 `shielded_pool_withdraw` | 5,402 | 7 | 920 ms | 180 ms | 117,578 |
| 04 `shielded_transact` | 26,313 | 21 | 3,736 ms | 896 ms | 190,615–203,471 |

Constraints from `xark inspect`. Proving in node 24 through the generated clients, median of three,
on an Apple M4 Max; "8 workers" is xark-wasm's worker pool. Compute units are the whole instruction
in LiteSVM, so 03 and 04 include the accounts they create and the tokens they move. 04 varies with
its public inputs, past the default 200k limit, so its client asks for 250k. A proof is always 256
bytes.

## Testing

Each example's `client/*.test.ts` is its reference app. It proves with the generated client in
node, then sends the proof to the example's program in [LiteSVM](https://github.com/LiteSVM/litesvm),
verified through the same `alt_bn128` syscalls mainnet uses, with no validator and no network. The
tests cover the happy path and the refusals: a tampered proof, a proof for yesterday, a proof
submitted by someone else, a stranger's note, a redirected withdrawal or fee, a double spend. When inputs can't be proved, the client says which
line of the circuit fails (`CheckFailed`, with the source line). Each circuit also has its own Rust
tests (`just test-circuits`).

```
 ✓ client/over-9000.test.ts (2 tests)
 ✓ client/age-verification.test.ts (2 tests)
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
a public input no constraint uses, dev-mode keys blocked from export); `xark profile` to attribute
every constraint to a source line; and a Pinocchio low-CU verify path.

## Status & safety

> Reference implementations for **learning: unaudited, not for production.** The shielded examples
> especially are educational; privacy tooling carries real legal weight depending on your
> jurisdiction. Never ship dev-mode keys; use `xark ceremony` for anything real.

## Resources

- [xark](https://github.com/blueshift-gg/xark): `docs/subset.md`, `docs/integer-ops.md`, `docs/trusted-setup.md`, `docs/security.md`
- [Learn ZK, from zero](./docs/learn-zk/)

## License

MIT, see [LICENSE](./LICENSE).
