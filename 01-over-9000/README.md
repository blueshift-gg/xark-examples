# 01 · Over 9000 — your first ZK range proof

> Goal: prove *"my secret power level is over 9000"* to a Solana program —
> without revealing the number.

This is the smallest useful thing you can do with a zero-knowledge proof: convince a verifier
that a hidden number satisfies a bound. It's silly on purpose. Once you've got this running,
examples [02](../02-age-verification/) and [03](../03-shielded-pool/) point the exact same
mechanism at real problems.

New to any of these words (circuit, witness, proof, verifying key)? Read
[docs/learn-zk](../docs/learn-zk/) first — it's a 20-minute from-zero primer.

## The circuit

```noir
fn main(power_level: u64) {
    assert(power_level > 9000);
}
```

That's the whole thing. Three things are happening:

- **`power_level` is private.** It's a *witness* — an input to the proof that never appears in the
  proof itself. The verifier never sees it.
- **`u64` is a range constraint.** Declaring the type forces `power_level` into `[0, 2^64)`. The
  `> 9000` comparison compiles down to ACIR's `RANGE` black-box, which xark lowers into R1CS and
  proves. *That is the entire mechanism behind every range proof.*
- **Zero public inputs.** The threshold `9000` is fixed inside the circuit, so it's baked into the
  verifying key. Nothing but the 256-byte proof goes on-chain.

## What it proves (and what it doesn't)

- It proves the sender knows *some* `power_level > 9000`.
- It does **not** tie that number to anything — any prover can just pick `9001`.

That's the point of a bare range proof: it's a *mechanism*, not an application. Bind the hidden
value to something the world cares about — a commitment (example 02) or a Merkle tree of deposits
(example 03) — and the same range logic becomes an age gate or a private balance check.

## Run it

Prerequisites: `nargo` 1.0.0-beta.22, the `xark` CLI, and (to deploy) the Anza CLI. Full setup is
in [../RUNBOOK.md](../RUNBOOK.md).

```bash
# 1. Prove locally: compile → witness → keys (dev mode) → prove → verify
just prove
# Expect: "Proof verified: true"

# 2. Generate the on-chain verifier crate (VK baked in)
just export

# 3. Build the program
just build-program

# 4. Verify the proof in a real Solana VM (no deploy needed)
cd ../e2e && cargo test over_9000
# Expect: over_9000_verifies_on_chain ... ok
```

To deploy to devnet, `solana program deploy` the `.so` and submit the 256-byte
`instruction_data.bin` with any Solana client.

## Files

```
circuit/          the Noir circuit (src/main.nr), inputs (Prover.toml)
program/          Pinocchio on-chain verifier — one verify call, minimal CU
justfile          the pipeline: prove · export · build-program
```

## Try this

- Change `power_level` in `Prover.toml` to `9000` and re-run `just prove` — proving fails, because
  the witness no longer satisfies the circuit. That failure *is* the guarantee.
- Run `just verify-snarkjs` to verify the very same proof in JavaScript. xark emits
  snarkjs-compatible artifacts, so a proof made for Solana also verifies in a browser.

Next: [**02 · Age verification →**](../02-age-verification/) — make the hidden number *mean*
something by binding it to a commitment.
