// Submit the xark-generated proof to the deployed on-chain verifier program.
//
// `instruction_data.bin` is produced by `just export` and is exactly the
// 256-byte proof (this circuit has no public inputs). The program returns
// success iff the proof verifies.
//
// Env:
//   RPC_URL     defaults to devnet
//   PROGRAM_ID  the deployed program id (required)
//   IX_DATA     path to instruction_data.bin
//   PAYER       path to a funded keypair json (defaults to the Solana CLI id)
import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import {
  Connection,
  Keypair,
  PublicKey,
  Transaction,
  TransactionInstruction,
  sendAndConfirmTransaction,
  clusterApiUrl,
} from "@solana/web3.js";

const RPC = process.env.RPC_URL ?? clusterApiUrl("devnet");
const PROGRAM_ID = new PublicKey(
  process.env.PROGRAM_ID ?? "REPLACE_WITH_DEPLOYED_PROGRAM_ID",
);
const IX_DATA =
  process.env.IX_DATA ??
  "../circuit/target/over_9000-xark-verifier/instruction_data.bin";
const PAYER = process.env.PAYER ?? `${homedir()}/.config/solana/id.json`;

async function main() {
  const data = readFileSync(IX_DATA);
  const payer = Keypair.fromSecretKey(
    new Uint8Array(JSON.parse(readFileSync(PAYER, "utf8"))),
  );
  const connection = new Connection(RPC, "confirmed");

  const ix = new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [], // no accounts — verification is pure
    data,
  });

  const sig = await sendAndConfirmTransaction(
    connection,
    new Transaction().add(ix),
    [payer],
  );
  console.log(`It's over 9000! Proof verified on-chain.\nSignature: ${sig}`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
