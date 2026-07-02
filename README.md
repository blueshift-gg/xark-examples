# xark Examples

Zero-knowledge circuits written in [Noir](https://noir-lang.org/), proved and verified on
[Solana](https://solana.com/) with [Groth16](https://eprint.iacr.org/2016/260) via
[**xark**](https://github.com/blueshift-gg/xark). Four worked examples, easiest to hardest, each
introducing one new idea — and every one is **built, proved, and verified end-to-end in an
in-process Solana VM** (no validator, no network; see [Testing](#testing)).

New to zero-knowledge? Start with the [from-zero primer](./docs/learn-zk/) — the mental model before
any Noir.

## The idea in one paragraph

A circuit is a computation you can *prove you ran correctly* without revealing its inputs. You write
it in Noir, mark each input `pub` (public) or leave it private, and assert the constraints that must
hold. xark compiles that circuit to a ~256-byte Groth16 proof and generates a Solana verifier that
checks it through the native `alt_bn128` syscalls. The payoff: a program can act on *"this statement
is true"* — you're over 18, this note is unspent, the amounts balance — without ever seeing the
private data behind it. Designing a ZK application is mostly choosing **what's private, what's public,
and what must hold between them**; the examples below are that choice made four ways.

## The examples

| Example | Proves | New idea | Public inputs | On-chain |
|---|---|---|:-:|---|
| [01 · over-9000](./01-over-9000/) | a secret value is `> 9000` | range proofs, bare verify | 0 | Pinocchio |
| [02 · age-verification](./02-age-verification/) | `age ≥ 18` without a birthday | commitments | 2 | Pinocchio |
| [03 · shielded-pool](./03-shielded-pool/) | an unlinkable deposit → withdrawal (multi-denomination Tornado++) | Merkle membership, nullifiers, two circuits | 4 + 7 | Anchor |
| [04 · shielded-transfer](./04-shielded-transfer/) | a private, arbitrary-amount payment (Zcash-style) | notes, key hierarchy, JoinSplit, SPL | 12 | Anchor + SPL |

## How it fits together

```
  Noir circuit      you write the rules: what's secret, what's public, what must hold
     │ nargo execute
     ▼
  ACIR + witness    a compiled circuit + one satisfying secret assignment
     │ xark          lower ACIR → R1CS, then Groth16 over BN254
     ▼
  proof + verifier  a 256-byte proof, and `xark export`s a verifier crate with the VK baked in
     │
     ▼
  Solana program    calls verify_instruction_data(...) via the native alt_bn128 syscalls
```

Noir is the frontend (the language); **xark is the backend** (proving + the on-chain verifier). To
change a circuit you edit Noir and re-run `xark export` — your program depends on the generated crate
and doesn't itself change. `instruction_data.bin` is just `proof (256 B) ‖ public_inputs (N × 32 B,
little-endian)`.

## Run one

```bash
cd 01-over-9000
just prove          # nargo execute → xark setup → prove → verify
just export         # → the verifier crate + instruction_data.bin
just build-program  # → the deployable .so
```

Each example's README walks its circuit; [RUNBOOK.md](./RUNBOOK.md) has the full toolchain, the
one-time `xark-verifier` path setup, and devnet deployment.

## Testing

The [`e2e/`](./e2e/) suite loads each compiled program into
[LiteSVM](https://github.com/LiteSVM/litesvm) and submits a real proof — verified through the same
`alt_bn128` syscalls mainnet uses, with no validator and no network. Every test also submits a
tampered proof and asserts it's rejected; 03 runs a full deposit → withdraw and asserts a
double-spend fails; 04 runs a shield → private-pay → withdraw.

```bash
# build the examples first (see each README / the RUNBOOK), then:
cd e2e && cargo test          # or, from the repo root: just test
# over_9000_verifies_on_chain ... ok
# age_verification_verifies_on_chain ... ok
# shielded_pool_full_flow ... ok
# shielded_transfer_flow ... ok
```

## How xark compares

The Solana Foundation's [`noir-examples`](https://github.com/solana-foundation/noir-examples) fill
this backend slot with **Sunspot**. xark differs where it matters for shipping: a real multi-party
setup (`xark ceremony`) instead of dev keys, Lean formal proofs plus fuzzing and differential tests
against snarkjs, **explicit opcode rejection** (it refuses circuits it can't prove soundly rather
than emit an unsound proof), snarkjs-compatible artifacts, and a Pinocchio low-CU verify path.

## Status & safety

> Reference implementations for **learning — unaudited, not for production.** The shielded examples
> especially are educational; privacy tooling carries real legal weight depending on your
> jurisdiction. Never ship `--insecure-dev-mode` keys — use `xark ceremony` for anything real.

## Resources

- [Noir documentation](https://noir-lang.org/docs)
- [xark](https://github.com/blueshift-gg/xark) — `docs/architecture.md`, `docs/trusted-setup.md`, `docs/security.md`
- [Learn ZK, from zero](./docs/learn-zk/)

## License

MIT — see [LICENSE](./LICENSE).
