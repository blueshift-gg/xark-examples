// A "note" is the secret you keep after depositing. Whoever holds it can
// withdraw exactly once. Guard it like a bearer bond — losing it loses the
// funds; leaking it lets someone else withdraw.
import { randomBytes } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";

// BN254 scalar field modulus (the field Noir/xark operate over).
export const FIELD_MODULUS =
  21888242871839275222246405745257275088548364400416034343698204186575808495617n;

export interface Note {
  secret: bigint;
  nullifier: bigint;
}

/** A uniformly random field element. */
export function randomFieldElement(): bigint {
  return BigInt("0x" + randomBytes(32).toString("hex")) % FIELD_MODULUS;
}

export function newNote(): Note {
  return { secret: randomFieldElement(), nullifier: randomFieldElement() };
}

export function saveNote(note: Note, path: string): void {
  writeFileSync(
    path,
    JSON.stringify(
      { secret: note.secret.toString(), nullifier: note.nullifier.toString() },
      null,
      2,
    ),
  );
}

export function loadNote(path: string): Note {
  const j = JSON.parse(readFileSync(path, "utf8"));
  return { secret: BigInt(j.secret), nullifier: BigInt(j.nullifier) };
}
