// Poseidon2 over BN254, via Barretenberg (bb.js).
//
// This MUST produce the same hash as the Noir circuits, or roots and
// commitments won't match and every proof fails. We use `@aztec/bb.js` because
// Barretenberg is exactly the backend Noir's `std::hash::poseidon2` compiles
// against — so the parameters are identical by construction.
//
// ⚠️ Pin bb.js to the version matching your nargo (1.0.0-beta.22). If you bump
// nargo, re-check that this still agrees with the circuit (there's a
// `differential` check in the RUNBOOK).
import { Barretenberg, Fr } from "@aztec/bb.js";

let apiPromise: Promise<Barretenberg> | null = null;

async function api(): Promise<Barretenberg> {
  if (!apiPromise) apiPromise = Barretenberg.new();
  return apiPromise;
}

/** Poseidon2 hash of `inputs` (any arity) → a field element as bigint. */
export async function poseidon2(inputs: bigint[]): Promise<bigint> {
  const bb = await api();
  const out = await bb.poseidon2Hash(inputs.map((x) => new Fr(x)));
  return BigInt(out.toString());
}

export const hash2 = (a: bigint, b: bigint) => poseidon2([a, b]);
export const hash1 = (a: bigint) => poseidon2([a]);

/** Release the Barretenberg worker (call once at process exit). */
export async function shutdown(): Promise<void> {
  if (apiPromise) await (await apiPromise).destroy();
}
