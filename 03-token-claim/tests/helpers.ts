// What the test needs beyond `@blueshift-gg/xark/testing`.

/** A `u64`, little-endian, as the program reads one. */
export function u64le(x: bigint): Uint8Array {
  const out = new Uint8Array(8);
  new DataView(out.buffer).setBigUint64(0, x, true);
  return out;
}
