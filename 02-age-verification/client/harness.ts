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
import { FailedTransactionMetadata, LiteSVM, type TransactionMetadata } from "litesvm";
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
