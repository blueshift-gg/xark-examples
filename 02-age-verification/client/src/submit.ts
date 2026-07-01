// Submit the age proof to the deployed on-chain verifier program.
//
// instruction_data.bin (from `just export`) =
//   proof (256 B) || commitment (32 B) || current_year (32 B)
// The program verifies the proof and sanity-checks current_year.
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
  "../circuit/target/age_verification-xark-verifier/instruction_data.bin";
const PAYER = process.env.PAYER ?? `${homedir()}/.config/solana/id.json`;

async function main() {
  const data = readFileSync(IX_DATA);
  const payer = Keypair.fromSecretKey(
    new Uint8Array(JSON.parse(readFileSync(PAYER, "utf8"))),
  );
  const connection = new Connection(RPC, "confirmed");

  const ix = new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [],
    data,
  });

  const sig = await sendAndConfirmTransaction(
    connection,
    new Transaction().add(ix),
    [payer],
  );
  console.log(`Verified 18+ on-chain — no birthday revealed.\nSignature: ${sig}`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
