// Shielded transfer — JoinSplit-style `transact` circuit (2-in / 2-out).
//
// One primitive covers deposit, private transfer, and withdraw via public value
// slots: `vpub_in` (public value entering the shield, e.g. an SPL deposit) and
// `vpub_out` (public value leaving, e.g. a withdraw). Fully-shielded transfers
// set both to 0. Inputs can be "dummy" (value 0, membership skipped) so a
// deposit needs no real inputs.
//
// The circuit proves, in one shot:
//   - spend side: input notes are real leaves (membership vs the anchor `root`),
//     only the owner can spend (key derivation), each burns a position-bound
//     nullifier;
//   - output side: outputs are well-formed commitments AND are correctly
//     appended to the current tree (`old_root` → `new_root` via a private
//     frontier), so the on-chain program never hashes (03's trick, ×2 leaves);
//   - value is conserved.
//
// Note *delivery* (the encrypted memo) is off-circuit: the sender publishes
// (epk, ciphertext) as event data and the recipient trial-decrypts — Zcash's
// mechanism. Funds security (commitments + nullifiers) is fully in-circuit.
//
// Public inputs, in order:
//   root, asset, nf[0], nf[1], cm_out[0], cm_out[1],
//   old_root, new_root, insert_index, vpub_in, vpub_out, fee

global TREE_DEPTH: u32 = 20;
global NINS: u32 = 2;
global NOUTS: u32 = 2;
global RATE: u32 = 3;
global ZERO: Field = 0; // empty-leaf sentinel

global D_NK: Field = 1;
global D_IVK: Field = 2;
global D_ADDR: Field = 3;
global D_CM: Field = 4;
global D_NF: Field = 5;

// Poseidon2 sponge over BN254 (width 4, rate 3, capacity 1). Capacity carries
// the arity as a domain tag; fixed-length messages so no 10* padding needed.
fn hash<let N: u32>(inputs: [Field; N]) -> Field {
    let mut state: [Field; 4] = [0, 0, 0, 0];
    state[3] = N as Field;
    for i in 0..N {
        let lane = i % RATE;
        state[lane] += inputs[i];
        if lane == RATE - 1 {
            state = std::hash::poseidon2_permutation(state);
        }
    }
    if N % RATE != 0 {
        state = std::hash::poseidon2_permutation(state);
    }
    state[0]
}

fn derive_nk(sk: Field) -> Field {
    hash([sk, D_NK])
}
fn derive_ivk(sk: Field) -> Field {
    hash([sk, D_IVK])
}
fn derive_addr(ivk: Field, d: Field) -> Field {
    hash([ivk, d, D_ADDR])
}
fn note_commit(value: Field, asset: Field, addr: Field, rho: Field, rseed: Field) -> Field {
    hash([value, asset, addr, rho, rseed, D_CM])
}
fn nullifier(nk: Field, cm: Field, pos: Field) -> Field {
    hash([nk, cm, pos, D_NF])
}
fn hash2(l: Field, r: Field) -> Field {
    hash([l, r])
}

// Merkle membership: fold a leaf up with its authentication path.
fn merkle_root(leaf: Field, path: [Field; TREE_DEPTH], index: [Field; TREE_DEPTH]) -> Field {
    let mut cur = leaf;
    for i in 0..TREE_DEPTH {
        let b = index[i];
        assert(b * (b - 1) == 0);
        let l = if b == 1 { path[i] } else { cur };
        let r = if b == 1 { cur } else { path[i] };
        cur = hash2(l, r);
    }
    cur
}

// zeros[i] = root of an all-empty subtree of height i.
fn zeros_array() -> [Field; TREE_DEPTH] {
    let mut z: [Field; TREE_DEPTH] = [0; TREE_DEPTH];
    z[0] = ZERO;
    for i in 1..TREE_DEPTH {
        z[i] = hash2(z[i - 1], z[i - 1]);
    }
    z
}

// Current tip: root when position `index` holds `leaf` and everything after is
// empty, given the left-sibling frontier. (Binds the private frontier.)
fn root_with_leaf(leaf: Field, index: u32, filled: [Field; TREE_DEPTH], zeros: [Field; TREE_DEPTH]) -> Field {
    let mut current = leaf;
    let mut idx = index;
    for i in 0..TREE_DEPTH {
        if idx & 1 == 0 {
            current = hash2(current, zeros[i]);
        } else {
            current = hash2(filled[i], current);
        }
        idx = idx >> 1;
    }
    current
}

// Incremental insert (Tornado MerkleTreeWithHistory): append `leaf` at `index`,
// returning the updated frontier and the new root.
fn insert(leaf: Field, index: u32, filled: [Field; TREE_DEPTH], zeros: [Field; TREE_DEPTH]) -> ([Field; TREE_DEPTH], Field) {
    let mut new_filled = filled;
    let mut current = leaf;
    let mut idx = index;
    for i in 0..TREE_DEPTH {
        if idx & 1 == 0 {
            new_filled[i] = current;
            current = hash2(current, zeros[i]);
        } else {
            current = hash2(filled[i], current);
        }
        idx = idx >> 1;
    }
    (new_filled, current)
}

fn range_u64(x: Field) {
    assert(x as u64 as Field == x);
}

fn main(
    sk: Field,
    in_value: [Field; NINS],
    in_d: [Field; NINS],
    in_rho: [Field; NINS],
    in_rseed: [Field; NINS],
    in_path: [[Field; TREE_DEPTH]; NINS],
    in_index: [[Field; TREE_DEPTH]; NINS],
    in_enforce: [Field; NINS], // 1 = real spend, 0 = dummy (value 0, membership skipped)
    out_value: [Field; NOUTS],
    out_addr: [Field; NOUTS],
    out_rho: [Field; NOUTS],
    out_rseed: [Field; NOUTS],
    filled_subtrees: [Field; TREE_DEPTH], // private frontier at insert_index
    root: pub Field,
    asset: pub Field,
    nf: pub [Field; NINS],
    cm_out: pub [Field; NOUTS],
    old_root: pub Field,
    new_root: pub Field,
    insert_index: pub u32,
    vpub_in: pub Field,
    vpub_out: pub Field,
    fee: pub Field,
) {
    let nk = derive_nk(sk);
    let ivk = derive_ivk(sk);

    // ---- spend side ----
    let mut sum_in: Field = 0;
    for i in 0..NINS {
        let e = in_enforce[i];
        assert(e * (e - 1) == 0);
        range_u64(in_value[i]);
        assert(in_value[i] * (1 - e) == 0); // dummy inputs carry value 0

        let addr = derive_addr(ivk, in_d[i]);
        let cm = note_commit(in_value[i], asset, addr, in_rho[i], in_rseed[i]);

        let mut pos: Field = 0;
        let mut pow: Field = 1;
        for k in 0..TREE_DEPTH {
            pos += in_index[i][k] * pow;
            pow *= 2;
        }

        let computed = merkle_root(cm, in_path[i], in_index[i]);
        assert((computed - root) * e == 0); // membership only for real inputs
        assert(nullifier(nk, cm, pos) == nf[i]);
        sum_in += in_value[i];
    }

    // ---- output side: commitments + append to the tree ----
    let mut sum_out: Field = 0;
    for j in 0..NOUTS {
        range_u64(out_value[j]);
        let cm = note_commit(out_value[j], asset, out_addr[j], out_rho[j], out_rseed[j]);
        assert(cm == cm_out[j]);
        sum_out += out_value[j];
    }

    let zeros = zeros_array();
    assert((insert_index as Field + 1) as u64 as Field == insert_index as Field + 1); // fits u64
    // bind the frontier to the current tip, then append the two outputs
    assert(root_with_leaf(ZERO, insert_index, filled_subtrees, zeros) == old_root);
    let (f1, _r1) = insert(cm_out[0], insert_index, filled_subtrees, zeros);
    let (_f2, computed_new_root) = insert(cm_out[1], insert_index + 1, f1, zeros);
    assert(computed_new_root == new_root);

    // ---- value conservation ----
    range_u64(vpub_in);
    range_u64(vpub_out);
    range_u64(fee);
    assert(sum_in + vpub_in == sum_out + vpub_out + fee);
}

// Correctness tests + witness generation (`nargo test`) live in a separate
// module so this file is only the circuit. See src/tests.nr.
mod tests;
