# xark Examples

Zero-knowledge circuits written in **ordinary Rust**, proved and verified on
[Solana](https://solana.com/) with [Groth16](https://eprint.iacr.org/2016/260) via
[**xark**](https://github.com/blueshift-gg/xark). Four worked examples, easiest to hardest, each
introducing one new idea — and every one is **built, proved, and verified end-to-end in an
in-process Solana VM** (no validator, no network; see [Testing](#testing)).

New to zero-knowledge? Start with the [from-zero primer](./docs/learn-zk/) — the mental model before
any code.

## The idea in one paragraph

A circuit is a computation you can *prove you ran correctly* without revealing its inputs. With
xark you write it as a plain Rust function — `Private<Field>` / `Public<Field>` mark input
visibility, `assert*` state the constraints that must hold — and the compiler drives rustc itself,
extracts the MIR, and lowers it to R1CS. No separate circuit language: gadgets (Poseidon2, Merkle
trees, …) are ordinary Rust crates you import, and this repo's circuits share one the same way.
xark then produces a ~256-byte Groth16 proof and generates a Solana verifier crate that checks it
through the native `alt_bn128` syscalls. The payoff: a program can act on *"this statement is
true"* — you're over 18, this note is unspent, the amounts balance — without ever seeing the
private data behind it. Designing a ZK application is mostly choosing **what's private, what's
public, and what must hold between them**; the examples below are that choice made four ways.

## The examples

| Example | Proves | New idea | Public inputs | On-chain |
|---|---|---|:-:|---|
| [01 · over-9000](./01-over-9000/) | a secret value is `> 9000` | range proofs, bare verify | 0 | Pinocchio |
| [02 · age-verification](./02-age-verification/) | `age ≥ 18` without a birthday | commitments | 2 | Pinocchio |
| [03 · shielded-pool](./03-shielded-pool/) | an unlinkable deposit → withdrawal (multi-denomination Tornado++) | Merkle membership, nullifiers, two circuits | 4 + 7 | Anchor |
| [04 · shielded-transfer](./04-shielded-transfer/) | a private, arbitrary-amount payment (Zcash-style) | notes, key hierarchy, JoinSplit, SPL | 18 | Anchor + SPL |

## How it fits together

```
  Rust circuit      you write the rules: what's secret, what's public, what must hold
     │ xark build     (xark drives rustc → MIR → xark-IR → R1CS)
     ▼
  circuit + R1CS    target/xark/<name>/{circuit,r1cs}.json
     │ xark setup / prove --input-file ...   (solves the witness, proves, self-checks)
     ▼
  proof + verifier  a 256-byte proof; `xark export` emits a verifier crate with the VK baked in
     │
     ▼
  Solana program    calls verify_instruction_data(...) via the native alt_bn128 syscalls
```

One language end to end: the circuit, the gadget libraries, the wallet-side witness code, and the
program are all Rust. To change a circuit without changing its public-input contract, edit it and
re-run `just export`; the program continues to depend on the generated crate.
`instruction_data.bin` is just
`proof (256 B) ‖ public_inputs (N × 32 B, little-endian)`.

## Run one

```bash
cd 01-over-9000
just build-program  # circuit → dev setup → proof → verifier crate → deployable .so
just test-circuit   # optional circuit pass/fail tests
```

Each example's README walks its circuit; [RUNBOOK.md](./RUNBOOK.md) has the full toolchain
(`xark doctor` checks it), how the xark dependency wiring works, and devnet deployment.

## Testing

The [`e2e/`](./e2e/) suite loads each compiled program into
[LiteSVM](https://github.com/LiteSVM/litesvm) and submits a real proof — verified through the same
`alt_bn128` syscalls mainnet uses, with no validator and no network. The verifier and pool suites
exercise tampered proofs and rejection paths; 03 runs a full deposit → withdraw and asserts a
double-spend fails; 04 runs a shield → private-pay → withdraw. The same crate doubles as the
examples' tiny **wallet SDK**: a native Poseidon2 pinned to the circuit gadget's known-answer
vector, Merkle/note helpers, and the witness generators the justfiles call.

```bash
# build the examples first (see each README / the RUNBOOK), then:
cd e2e && cargo test          # or, from the repo root: just test
# over_9000_verifies_on_chain ... ok
# age_verification_verifies_on_chain ... ok
# shielded_pool_full_flow ... ok
# shielded_transfer_flow ... ok
```

## How xark compares

The Solana Foundation's [`noir-examples`](https://github.com/solana-foundation/noir-examples) pair
Noir with **Sunspot** for this slot. xark takes a different developer path: circuits are plain
Rust (one language for circuit, gadgets, wallet, and program — with rust-analyzer diagnostics via
`xark check`), a real multi-party setup (`xark ceremony`) instead of dev keys, Lean formal proofs
plus fuzzing and differential tests against snarkjs, guard rails that refuse what can't be proved
soundly (subset rejection with rustc spans; dev-mode keys blocked from export), snarkjs-compatible
artifacts, a `xark profile` that attributes every constraint to a source line, and a Pinocchio
low-CU verify path.

## Status & safety

> Reference implementations for **learning — unaudited, not for production.** The shielded examples
> especially are educational; privacy tooling carries real legal weight depending on your
> jurisdiction. Never ship dev-mode keys — use `xark ceremony` for anything real.

## Resources

- [xark](https://github.com/blueshift-gg/xark) — `docs/architecture.md`, `docs/integer-ops.md`, `docs/trusted-setup.md`, `docs/security.md`
- [Learn ZK, from zero](./docs/learn-zk/)

## License

MIT — see [LICENSE](./LICENSE).
