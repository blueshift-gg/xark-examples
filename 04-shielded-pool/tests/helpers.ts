// What the tests need beyond `@blueshift-gg/xark/testing`: proving on a pool of
// threads, and the program's instruction data.
import * as xark from "@blueshift-gg/xark";
import { createPool } from "@blueshift-gg/xark/pool";

/**
 * xark proving across a pool of worker threads: pass `{ wasm }` to a client's
 * `prover`, and `terminate` when done. The same proofs and errors, about 4× faster
 * on 8 threads for 06's circuit. `proveStreaming` streams the constraint rows from
 * `circuit.xbc`, so the keys are made `--r1cs-form reduced` (`xark build`'s default). In a browser the
 * pool needs a cross-origin-isolated page; see `@blueshift-gg/xark`'s README.
 */
export async function pooled(threads = 8) {
  const pool = await createPool({ threads });
  const prove: typeof xark.prove = (circuit, provingKey, inputs, sources) =>
    xark.proveStreaming(circuit, provingKey, inputs, { sources, pool });
  return { wasm: { ...xark, prove }, terminate: () => pool.terminate() };
}

/**
 * An instruction's data: its one-byte tag (see `programs/shielded-pool/src/lib.rs`),
 * then its arguments. Fixed-size arguments (`[u8; N]`, `u64`) are their bytes;
 * `u64le` writes one.
 */
export function instructionData(tag: number, ...args: Uint8Array[]): Uint8Array {
  return new Uint8Array(Buffer.concat([Uint8Array.of(tag), ...args]));
}

/** A `u64`, little-endian, as the program reads it. */
export function u64le(x: bigint): Uint8Array {
  const out = new Uint8Array(8);
  new DataView(out.buffer).setBigUint64(0, x, true);
  return out;
}
