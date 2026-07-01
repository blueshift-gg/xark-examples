// Prepare circuit inputs for the shielded pool.
//
//   tsx src/prepare.ts empty-root
//       → prints zeros[H]; pass it to the program's `initialize`.
//
//   tsx src/prepare.ts deposit
//       → makes a new note, computes the commitment + new_root + frontier,
//         writes ../circuits/deposit/Prover.toml, saves the note, and prints
//         the `deposit` instruction args (commitment, new_root).
//
//   tsx src/prepare.ts withdraw <note.json> <recipient> <relayer> <fee>
//       → rebuilds the Merkle path for your note, writes
//         ../circuits/withdraw/Prover.toml, and prints the `withdraw`
//         instruction args (root, nullifier_hash, fee).
//
// After writing a Prover.toml, run the example's `just prove-deposit` /
// `just prove-withdraw` to produce the proof + instruction_data, then submit.
//
// State: this demo keeps the commitment list in ./state/leaves.json. A real
// client scans on-chain `Deposit` events instead. Never rely on local state in
// production.
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { PublicKey } from "@solana/web3.js";
import { hash1, hash2, shutdown } from "./poseidon.js";
import { MerkleTree } from "./merkle.js";
import { loadNote, newNote, saveNote, type Note } from "./note.js";

const H = 20;
const STATE = "state";
const LEAVES = `${STATE}/leaves.json`;
const DEPOSIT_PROVER = "../circuits/deposit/Prover.toml";
const WITHDRAW_PROVER = "../circuits/withdraw/Prover.toml";

function loadLeaves(): bigint[] {
  if (!existsSync(LEAVES)) return [];
  return (JSON.parse(readFileSync(LEAVES, "utf8")) as string[]).map(BigInt);
}
function saveLeaves(leaves: bigint[]): void {
  mkdirSync(STATE, { recursive: true });
  writeFileSync(LEAVES, JSON.stringify(leaves.map((l) => l.toString()), null, 2));
}

const dec = (x: bigint) => `"${x.toString()}"`;
const decArray = (xs: bigint[]) => `[${xs.map(dec).join(",")}]`;

/** 32-byte little-endian hex (the [u8;32] the Anchor instruction expects). */
function leHex(x: bigint): string {
  const bytes: number[] = [];
  let v = x;
  for (let i = 0; i < 32; i++) {
    bytes.push(Number(v & 0xffn));
    v >>= 8n;
  }
  return "0x" + bytes.map((b) => b.toString(16).padStart(2, "0")).join("");
}

/** Interpret a byte slice little-endian as a bigint field element. */
function leToBig(bytes: Uint8Array): bigint {
  let x = 0n;
  for (let i = 0; i < bytes.length; i++) x += BigInt(bytes[i]) << (8n * BigInt(i));
  return x;
}

/** Split a pubkey into (hi, lo): lo = bytes[0..16], hi = bytes[16..32]. */
function splitPubkey(pk: PublicKey): { hi: bigint; lo: bigint } {
  const b = pk.toBytes();
  return { lo: leToBig(b.slice(0, 16)), hi: leToBig(b.slice(16, 32)) };
}

async function emptyRoot(): Promise<void> {
  const tree = await MerkleTree.create(H, []);
  console.log("empty tree root (pass to `initialize` as empty_root):");
  console.log("  field :", tree.root.toString());
  console.log("  [u8;32]:", leHex(tree.root));
}

async function deposit(): Promise<void> {
  const leaves = loadLeaves();
  const tree = await MerkleTree.create(H, leaves);
  const oldRoot = tree.root;
  const index = tree.nextIndex;
  const frontier = tree.frontier();

  const note: Note = newNote();
  const commitment = await hash2(note.nullifier, note.secret);
  await tree.insert(commitment);
  const newRoot = tree.root;

  leaves.push(commitment);
  saveLeaves(leaves);
  mkdirSync(STATE, { recursive: true });
  const notePath = `${STATE}/note-${index}.json`;
  saveNote(note, notePath);

  const toml =
    `# Auto-written by prepare.ts (deposit at index ${index}).\n` +
    `filled_subtrees = ${decArray(frontier)}\n` +
    `old_root = ${dec(oldRoot)}\n` +
    `new_root = ${dec(newRoot)}\n` +
    `leaf = ${dec(commitment)}\n` +
    `index = "${index}"\n`;
  writeFileSync(DEPOSIT_PROVER, toml);

  console.log(`Note saved to ${notePath} — KEEP IT SAFE (it's your withdrawal key).`);
  console.log(`Wrote ${DEPOSIT_PROVER}. Now: cd .. && just prove-deposit`);
  console.log("\n`deposit` instruction args:");
  console.log("  commitment:", leHex(commitment));
  console.log("  new_root  :", leHex(newRoot));
}

async function withdraw(argv: string[]): Promise<void> {
  const [notePath, recipientStr, relayerStr, feeStr] = argv;
  if (!notePath || !recipientStr || !relayerStr || feeStr === undefined) {
    throw new Error("usage: withdraw <note.json> <recipient> <relayer> <fee>");
  }
  const note = loadNote(notePath);
  const recipient = new PublicKey(recipientStr);
  const relayer = new PublicKey(relayerStr);
  const fee = BigInt(feeStr);

  const leaves = loadLeaves();
  const tree = await MerkleTree.create(H, leaves);
  const commitment = await hash2(note.nullifier, note.secret);
  const index = tree.indexOf(commitment);
  if (index < 0) throw new Error("commitment not found in the tree (wrong note or stale state)");

  const root = tree.root;
  const { pathElements, pathIndices } = tree.path(index);
  const nullifierHash = await hash1(note.nullifier);
  const r = splitPubkey(recipient);
  const l = splitPubkey(relayer);

  const toml =
    `# Auto-written by prepare.ts (withdraw of leaf ${index}).\n` +
    `secret = ${dec(note.secret)}\n` +
    `nullifier = ${dec(note.nullifier)}\n` +
    `path_elements = ${decArray(pathElements)}\n` +
    `path_indices = ${decArray(pathIndices.map((b) => BigInt(b)))}\n` +
    `root = ${dec(root)}\n` +
    `nullifier_hash = ${dec(nullifierHash)}\n` +
    `recipient_hi = ${dec(r.hi)}\n` +
    `recipient_lo = ${dec(r.lo)}\n` +
    `relayer_hi = ${dec(l.hi)}\n` +
    `relayer_lo = ${dec(l.lo)}\n` +
    `fee = ${dec(fee)}\n`;
  writeFileSync(WITHDRAW_PROVER, toml);

  console.log(`Wrote ${WITHDRAW_PROVER}. Now: cd .. && just prove-withdraw`);
  console.log("\n`withdraw` instruction args:");
  console.log("  root          :", leHex(root));
  console.log("  nullifier_hash:", leHex(nullifierHash));
  console.log("  fee           :", fee.toString());
}

async function main(): Promise<void> {
  const [cmd, ...rest] = process.argv.slice(2);
  switch (cmd) {
    case "empty-root":
      await emptyRoot();
      break;
    case "deposit":
      await deposit();
      break;
    case "withdraw":
      await withdraw(rest);
      break;
    default:
      console.error("commands: empty-root | deposit | withdraw");
      process.exitCode = 1;
  }
  await shutdown();
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
