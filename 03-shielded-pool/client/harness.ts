// The test's harness: the circuit's build output, and a Solana VM to send
// transactions to. An app would fetch the build output from a URL
// (`Circuit.prover("/circuits/<name>/")`) and send to an RPC instead of LiteSVM; the
// clients and instructions are the same.
import {
  appendTransactionMessageInstructions,
  createTransactionMessage,
  generateKeyPairSigner,
  lamports,
  pipe,
  setTransactionMessageFeePayerSigner,
  setTransactionMessageLifetimeUsingBlockhash,
  signTransactionMessageWithSigners,
  type Address,
  type Instruction,
  type KeyPairSigner,
} from "@solana/kit";
import * as xark from "@blueshift-gg/xark-wasm";
import { createPool } from "@blueshift-gg/xark-wasm/pool";
import { FailedTransactionMetadata, LiteSVM, type TransactionMetadata } from "litesvm";
import { createHash } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

/** The example's directory. */
const root = join(import.meta.dirname, "..");

/** The build output of circuit `name` in crate `crate` (its `target/xark/<name>/`), as bytes. */
export function artifacts(crate: string, name: string) {
  const dir = join(root, crate, "target", "xark", name);
  const read = (f: string) => new Uint8Array(readFileSync(join(dir, f)));
  return {
    circuit: read("circuit.xbc"),
    provingKey: read("pk.bin"),
    verifyingKey: read("vk.bin"),
    sources: readFileSync(join(dir, "sources.json"), "utf8"),
    client: existsSync(join(dir, "client.wasm")) ? read("client.wasm") : undefined,
  };
}

/**
 * xark-wasm proving across a pool of worker threads: pass `{ wasm }` to a client's
 * `prover`, and `terminate` when done. The same proofs and errors, about 4× faster
 * on 8 threads for 04's circuit. `proveStreaming` streams the constraint rows from
 * `circuit.xbc`, so the keys are made `--r1cs-form reduced` (the justfiles do). In a browser the
 * pool needs a cross-origin-isolated page; see xark-wasm's README.
 */
export async function pooled(threads = 8) {
  const pool = await createPool({ threads });
  const prove: typeof xark.prove = (circuit, provingKey, inputs, sources) =>
    xark.proveStreaming(circuit, provingKey, inputs, { sources, pool });
  return { wasm: { ...xark, prove }, terminate: () => pool.terminate() };
}

/** A fresh VM with `program` (`<example>/program/target/deploy/<name>.so`) at `id`. */
export function vm(id: Address, program: string): LiteSVM {
  const svm = new LiteSVM();
  svm.addProgramFromFile(id, join(root, program));
  return svm;
}

/** A new keypair holding `sol` SOL. */
export async function funded(svm: LiteSVM, sol = 10n): Promise<KeyPairSigner> {
  const signer = await generateKeyPairSigner();
  svm.airdrop(signer.address, lamports(sol * 1_000_000_000n));
  return signer;
}

/**
 * Sign `instructions` with `payer` (and any signers they name) and send them. Throws
 * with the program's logs if the transaction fails, so a test can match the error.
 */
export async function send(svm: LiteSVM, payer: KeyPairSigner, ...instructions: Instruction[]): Promise<TransactionMetadata> {
  const tx = await pipe(
    createTransactionMessage({ version: 0 }),
    (m) => setTransactionMessageFeePayerSigner(payer, m),
    (m) => setTransactionMessageLifetimeUsingBlockhash({ blockhash: svm.latestBlockhash(), lastValidBlockHeight: 0n }, m),
    (m) => appendTransactionMessageInstructions(instructions, m),
    (m) => signTransactionMessageWithSigners(m),
  );
  const result = svm.sendTransaction(tx);
  // A new blockhash, so sending the same instructions again is a new transaction.
  svm.expireBlockhash();
  if (result instanceof FailedTransactionMetadata) {
    throw new Error(`transaction failed: ${result.err()}\n${result.meta().logs().join("\n")}`);
  }
  return result;
}

/**
 * An Anchor instruction's data: its discriminator, `sha256("global:<name>")[..8]`,
 * then its arguments' Borsh encodings. Fixed-size arguments (`[u8; N]`, `u64`) are
 * their bytes; `u64le` writes one.
 */
export function anchorData(name: string, ...args: Uint8Array[]): Uint8Array {
  const discriminator = createHash("sha256").update(`global:${name}`).digest().subarray(0, 8);
  return new Uint8Array(Buffer.concat([discriminator, ...args]));
}

/** A `u64`, little-endian, as Borsh and PDA seeds have it. */
export function u64le(x: bigint): Uint8Array {
  const out = new Uint8Array(8);
  new DataView(out.buffer).setBigUint64(0, x, true);
  return out;
}
