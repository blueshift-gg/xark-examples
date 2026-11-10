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

`xark client` generates [`client/over_9000.ts`](./client/over_9000.ts) from the circuit: its inputs
as TypeScript types (`powerLevel: U64`). Proving and sending is the whole app
([`client/over-9000.test.ts`](./client/over-9000.test.ts)):

```ts
const prover = await Over9000.prover("/circuits/over_9000/");
const proof = prover.prove({ powerLevel: 9001 });
// instruction data: proof.toBytes(), 256 bytes
```

The [program](./program/src/lib.rs) has one instruction, `Verify`, and no accounts: its instruction
data is the proof, parsed to exactly 256 bytes (else the built-in `InvalidInstructionData`), then
one call to the verifier crate `xark export` generated (a proof that fails is the program's own
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
npm run build          # circuit → key → verifier crate → client → .so
npm run test:circuit   # the circuit's Rust tests: 9001 passes, 9000 doesn't
npm test               # prove with the client, verify onchain (LiteSVM)
```

Prerequisites: [../RUNBOOK.md](../RUNBOOK.md) (`xark doctor` checks the Rust and xark side).

## Build it yourself, from `xark init`

The same example from nothing, in four steps. Every command below was run as written.

**1. The circuit.** `xark init` scaffolds a circuit crate, wired for rust-analyzer. Replace its
`src/lib.rs` with [the circuit above](./circuit/src/lib.rs) (tests included), then:

```bash
xark init over-9000
xark test over-9000                                    # builds the circuit, runs its tests
xark prove over-9000 --inputs '{"power_level": 9001}'  # makes the key on the first run
```

**2. The verifier and the client.** The verifier crate embeds the verifying key. The key from step 1
is a single-party setup from your machine's randomness, and the crate's `SETUP` constant says so;
for a key no single party has to be trusted for, run a [ceremony](../docs/ceremony.md).

```bash
xark export over-9000                           # over-9000/over-9000-verifier/
xark client over-9000 --out app/                # app/over_9000.ts
```

**3. The program.** `cargo new --lib program`, then add to its `Cargo.toml`:

```toml
[lib]
crate-type = ["cdylib"]

[dependencies]
over-9000-verifier = { path = "../over-9000/over-9000-verifier" }
pinocchio = "0.11"
```

and `src/lib.rs`, the [program](./program/src/lib.rs) without its doc comments:

```rust
#![no_std]
use over_9000_verifier::{Proof, verify};
use pinocchio::{
    AccountView, Address, ProgramResult,
    error::{ProgramError, ToStr},
    program_entrypoint,
};

program_entrypoint!(process_instruction);
pinocchio::nostd_panic_handler!();
pinocchio::no_allocator!();

fn process_instruction(_: &Address, _: &mut [AccountView], data: &[u8]) -> ProgramResult {
    let result = Verify::try_from(data).and_then(|verify| verify.process());
    if let Err(error @ ProgramError::Custom(_)) = &result {
        log(error.to_str::<Over9000Error>());
    }
    result
}

fn log(message: &str) {
    #[cfg(target_os = "solana")]
    unsafe { pinocchio::syscalls::sol_log_(message.as_ptr(), message.len() as u64) };
}

pub struct VerifyInstructionData<'a> {
    pub proof: &'a [u8; 256],
}

impl<'a> TryFrom<&'a [u8]> for VerifyInstructionData<'a> {
    type Error = ProgramError;

    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        let proof = data.try_into().map_err(|_| ProgramError::InvalidInstructionData)?;
        Ok(Self { proof })
    }
}

pub struct Verify<'a> {
    pub data: VerifyInstructionData<'a>,
}

impl<'a> TryFrom<&'a [u8]> for Verify<'a> {
    type Error = ProgramError;

    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        Ok(Self { data: VerifyInstructionData::try_from(data)? })
    }
}

impl Verify<'_> {
    // Out of line, or the pairing check's buffers take the entrypoint's frame past 4 KiB.
    #[inline(never)]
    pub fn process(&self) -> ProgramResult {
        if !verify(&Proof::from_le_bytes(self.data.proof), &[]) {
            return Err(Over9000Error::InvalidProof.into());
        }
        Ok(())
    }
}

#[repr(u32)]
pub enum Over9000Error {
    InvalidProof = 0,
}

impl From<Over9000Error> for ProgramError {
    fn from(e: Over9000Error) -> Self {
        ProgramError::Custom(e as u32)
    }
}

impl TryFrom<u32> for Over9000Error {
    type Error = ProgramError;

    fn try_from(code: u32) -> Result<Self, Self::Error> {
        match code {
            0 => Ok(Self::InvalidProof),
            _ => Err(ProgramError::InvalidArgument),
        }
    }
}

impl ToStr for Over9000Error {
    fn to_str(&self) -> &'static str {
        match self {
            Self::InvalidProof => "InvalidProof: invalid proof",
        }
    }
}
```

```bash
cd program && cargo build-sbf --arch v3         # program/target/deploy/over_9000_program.so
```

**4. The app.** In `app/`: `npm init -y`, `npm pkg set type=module`, `npm install
@blueshift-gg/xark @solana/kit litesvm tsx`, and `prove.ts`:

```ts
import { readFileSync } from "node:fs";
import {
  appendTransactionMessageInstruction, createTransactionMessage, generateKeyPairSigner, lamports, pipe,
  setTransactionMessageFeePayerSigner, setTransactionMessageLifetimeUsingBlockhash, signTransactionMessageWithSigners,
} from "@solana/kit";
import { FailedTransactionMetadata, LiteSVM } from "litesvm";
import { Over9000 } from "./over_9000";

// 1. Prove, with the circuit's build output (an app would fetch it from a URL).
const out = "../over-9000/target/xark/over-9000/";
const read = (f: string) => new Uint8Array(readFileSync(out + f));
const prover = await Over9000.prover(
  { circuit: read("circuit.xbc"), provingKey: read("pk.bin"), sources: readFileSync(out + "sources.json", "utf8") },
  { digest: Over9000.digest },
);
const proof = prover.prove({ powerLevel: 9001 });

// 2. Send it to the program, here in LiteSVM (an app would send to an RPC).
const svm = new LiteSVM();
const program = (await generateKeyPairSigner()).address;
svm.addProgramFromFile(program, "../program/target/deploy/over_9000_program.so");
const payer = await generateKeyPairSigner();
svm.airdrop(payer.address, lamports(1_000_000_000n));
const tx = await pipe(
  createTransactionMessage({ version: 0 }),
  (m) => setTransactionMessageFeePayerSigner(payer, m),
  (m) => setTransactionMessageLifetimeUsingBlockhash({ blockhash: svm.latestBlockhash(), lastValidBlockHeight: 0n }, m),
  (m) => appendTransactionMessageInstruction({ programAddress: program, data: proof.toBytes() }, m),
  (m) => signTransactionMessageWithSigners(m),
);
const result = svm.sendTransaction(tx);
console.log(result instanceof FailedTransactionMetadata ? `failed: ${result.err()}` : `verified onchain in ${result.computeUnitsConsumed()} CU`);
```

```bash
npx tsx prove.ts    # verified onchain in 75949 CU
```

That's the whole loop: change the circuit, rerun steps 1–2 and rebuild, and the client and the
verifier follow it. This directory's `npm run build` runs the same steps, with its key from `xark setup`.

## Layout

```
circuit/       the circuit crate: src/lib.rs, plain Rust, with its tests
client/        the generated TypeScript client, and its test: prove, then verify onchain
program/       Pinocchio verifier: src/lib.rs, one instruction, one verify call
package.json   build · test:circuit · test
```

Next → [02 · zKYC](../02-zkyc/): range proofs put to work
