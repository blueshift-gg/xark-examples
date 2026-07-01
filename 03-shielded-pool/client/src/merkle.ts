// Incremental Merkle tree that matches the Noir circuits exactly (same ZERO,
// same Poseidon2, same left/right ordering). It reproduces:
//   - the DEPOSIT circuit's `filled_subtrees` frontier, and
//   - the WITHDRAW circuit's authentication path.
//
// It recomputes all layers on every insert — O(n) hashing, fine for a demo.
// A production client would maintain the frontier incrementally and scan
// on-chain `Deposit` events instead of a local leaf list.
import { hash2 } from "./poseidon.js";

export const ZERO = 0n; // must equal `global ZERO` in the circuits

export class MerkleTree {
  readonly height: number;
  private zeros: bigint[] = [];
  private layers: bigint[][] = []; // layers[0] = leaves … layers[height] = [root]

  private constructor(height: number) {
    this.height = height;
  }

  static async create(height: number, leaves: bigint[] = []): Promise<MerkleTree> {
    const t = new MerkleTree(height);
    t.zeros[0] = ZERO;
    for (let i = 1; i <= height; i++) {
      t.zeros[i] = await hash2(t.zeros[i - 1], t.zeros[i - 1]);
    }
    t.layers[0] = [...leaves];
    await t.rebuild();
    return t;
  }

  private async rebuild(): Promise<void> {
    for (let level = 0; level < this.height; level++) {
      const cur = this.layers[level];
      const next: bigint[] = [];
      for (let i = 0; i < cur.length; i += 2) {
        const left = cur[i];
        const right = i + 1 < cur.length ? cur[i + 1] : this.zeros[level];
        next.push(await hash2(left, right));
      }
      this.layers[level + 1] = next;
    }
  }

  get root(): bigint {
    const top = this.layers[this.height];
    return top && top.length ? top[0] : this.zeros[this.height];
  }

  get nextIndex(): number {
    return this.layers[0].length;
  }

  async insert(leaf: bigint): Promise<number> {
    const index = this.layers[0].length;
    this.layers[0].push(leaf);
    await this.rebuild();
    return index;
  }

  indexOf(leaf: bigint): number {
    return this.layers[0].findIndex((l) => l === leaf);
  }

  /** The left-sibling frontier the DEPOSIT circuit consumes at `nextIndex`. */
  frontier(): bigint[] {
    const out: bigint[] = [];
    let index = this.nextIndex;
    for (let level = 0; level < this.height; level++) {
      if ((index & 1) === 1) {
        out.push(this.layers[level][index - 1] ?? this.zeros[level]);
      } else {
        out.push(this.zeros[level]); // unused by the circuit at this index
      }
      index = index >> 1;
    }
    return out;
  }

  /** Authentication path for the leaf at `index` (WITHDRAW circuit). */
  path(index: number): { pathElements: bigint[]; pathIndices: number[] } {
    const pathElements: bigint[] = [];
    const pathIndices: number[] = [];
    let idx = index;
    for (let level = 0; level < this.height; level++) {
      const isRight = idx & 1; // 1 → current node is the right child
      const siblingIdx = isRight ? idx - 1 : idx + 1;
      pathElements.push(this.layers[level][siblingIdx] ?? this.zeros[level]);
      pathIndices.push(isRight);
      idx = idx >> 1;
    }
    return { pathElements, pathIndices };
  }
}
