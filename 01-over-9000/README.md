# 01 · Over 9000: a range proof

Prove you know a secret number greater than 9000, revealing nothing else. It teaches the **range
proof**. A circuit computes in a prime field, which has no "less than": arithmetic wraps at `p`, so
`0 − 1` is the largest value there is. To compare numbers, the circuit first proves they're small,
by splitting each into bits and checking every bit; then order means what it should. That trick is
under every age, balance and price check in the later examples. This is the smallest circuit that
still does something real, and the whole xark loop in miniature: Rust circuit → Solana verifier →
TypeScript client → proof onchain.

## The circuit

```rust
use xark::prelude::*;

#[circuit]
pub fn over_9000(power_level: Private<u64>) {
    require(power_level > 9000u64);
}
```

A few lines, three ideas:

- **`Private` means witness.** `power_level` is an input the proof is computed from but never
  reveals. The verifier ends up convinced the requirement held without learning the number.
- **The width is a constraint.** A `u64` input is range-checked to `[0, 2⁶⁴)` on entry (a 64-bit
  decomposition in R1CS), and only then ordered. Every range proof (balances, ages, prices) is this
  one trick.
- **Zero public inputs.** `9000` is a literal, so it is fixed inside the verifying key. Only the
  256-byte proof reaches the chain.

It's ordinary Rust: `xark build` compiles it with your stable `rustc` and lowers it to constraints.
`xark check` surfaces anything outside the provable subset as a normal compiler diagnostic, in your
editor.

## The app

`xark build` generates [`clients/over-9000/ts/over_9000.ts`](./clients/over-9000/ts/over_9000.ts)
from the circuit: its inputs as TypeScript types (`powerLevel: U64`). Proving and sending is the
whole app ([`tests/over-9000.test.ts`](./tests/over-9000.test.ts)):

```ts
const prover = await Over9000.prover("/circuits/over_9000/");
const proof = prover.prove({ powerLevel: 9001 });
// instruction data: proof.toBytes(), 256 bytes
```

The [program](./programs/over-9000/src/lib.rs) has one instruction, `Verify`, and no accounts: its
instruction data is the proof, parsed to exactly 256 bytes (else the built-in
`InvalidInstructionData`), then one call to the verifier crate `xark build` generated
([`clients/over-9000/rust`](./clients/over-9000/rust/src/lib.rs)) (a proof that fails is the program's own
`Over9000Error::InvalidProof`, code 0, whose name it logs). Verifying costs about 76,000 compute units. Try
`prover.prove({ powerLevel: 9000 })`: there's no proof to make, and the client throws a
`CheckFailed` naming the line: ``the inputs fail the check at src/lib.rs:9:5: `require(power_level > 9000u64);` ``.

## What it does and doesn't prove

It proves someone knows *a* value `> 9000`. It doesn't tie that value to anything: a prover just
picks 9001. A bare range proof is a mechanism, not an application. Bind the hidden value to a
published commitment ([02](../02-zkyc/)) or a Merkle tree of deposits
([04](../04-shielded-pool/)) and the same check becomes a KYC gate or a private balance.

## Run it

```bash
xark build     # circuit, its clients (Rust verifier crate, TypeScript), then the program
xark test      # the circuit's Rust tests (9001 passes, 9000 doesn't), then prove and verify onchain
```

`npm run check` is CI's `xark build --check`: the committed keys and clients are what the source
builds. Prerequisites: [../RUNBOOK.md](../RUNBOOK.md) (`xark doctor` checks them).

## Build it yourself, from `xark init`

The same example from nothing. Every command below was run as written.

**1. The circuit.** `xark init` makes a project with a starter circuit in `circuits/over-9000/`.
Replace its `src/lib.rs` with [the circuit above](./circuits/over-9000/src/lib.rs) (tests included):

```bash
xark init over-9000 && cd over-9000
xark test                                     # builds the circuit, runs its tests
```

**2. The program.** `xark add program` makes the circuit's key and clients, then a Pinocchio program
that verifies its proofs, and a LiteSVM test. The key is a single-party setup from your machine's
randomness, and the verifier crate's `SETUP` constant says so; for a key no single party has to be
trusted for, run a [ceremony](../docs/ceremony.md).

```bash
xark add program over-9000    # programs/over-9000/, clients/over-9000/{rust,ts}/, keys/over-9000/
```

Replace `programs/over-9000/src/` with [this one's](./programs/over-9000/src/lib.rs): one file, one
instruction that takes only the proof, since the circuit has no public inputs. Its verify call:

```rust
use over_9000_verifier::{Proof, verify};

if !verify(&Proof::from_le_bytes(self.data.proof), []) {
    return Err(Over9000Error::InvalidProof.into());
}
```

**3. The app.** Replace `tests/over-9000.test.ts` with [this one](./tests/over-9000.test.ts): prove
with the generated client, send the proof to the program in LiteSVM.

```ts
import { circuit, funded, program as deployed, send, vm } from "@blueshift-gg/xark/testing";
import { Over9000 } from "../clients/over-9000/ts/over_9000";

const prover = await Over9000.prover(circuit("over-9000"), { digest: Over9000.digest });
const proof = prover.prove({ powerLevel: 9001 });
const svm = vm({ [program]: deployed("over-9000") });
await send(svm, await funded(svm), { programAddress: program, data: proof.toBytes() });
```

```bash
xark build                    # the circuit, its clients, then the program (cargo build-sbf)
npm install && xark test
```

That's the whole loop: change the circuit, run `xark build`, and the client and the verifier follow
it. Commit `keys/` and `clients/` with it: the verifier embeds the key.

## Layout

```
circuits/over-9000/   the circuit crate: src/lib.rs, plain Rust, with its tests
programs/over-9000/   Pinocchio verifier: src/lib.rs, one instruction, one verify call
clients/over-9000/    generated by `xark build`: the Rust verifier crate and the TypeScript client
keys/over-9000/       the proving and verifying keys (committed: the verifier embeds the key)
tests/                prove with the client, then verify onchain (LiteSVM)
xark.toml             the xark release the project is made with
```

Next → [02 · zKYC](../02-zkyc/): range proofs put to work
