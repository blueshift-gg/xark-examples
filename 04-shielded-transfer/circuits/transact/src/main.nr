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

// ---- tests ----------------------------------------------------------------

// Sparse 2-leaf tree at positions 0,1 built via `insert`, returning everything
// consistent: (zeros, tip/root, frontier@2, path0, path1, index0, index1).
fn two_leaf(
    cm0: Field,
    cm1: Field,
) -> ([Field; TREE_DEPTH], Field, [Field; TREE_DEPTH], [Field; TREE_DEPTH], [Field; TREE_DEPTH], [Field; TREE_DEPTH], [Field; TREE_DEPTH]) {
    let zeros = zeros_array();
    let (f1, _) = insert(cm0, 0, [0; TREE_DEPTH], zeros);
    let (f2, tip) = insert(cm1, 1, f1, zeros);
    let mut path0: [Field; TREE_DEPTH] = [0; TREE_DEPTH];
    let mut path1: [Field; TREE_DEPTH] = [0; TREE_DEPTH];
    path0[0] = cm1;
    path1[0] = cm0;
    for i in 1..TREE_DEPTH {
        path0[i] = zeros[i];
        path1[i] = zeros[i];
    }
    let index0: [Field; TREE_DEPTH] = [0; TREE_DEPTH];
    let mut index1: [Field; TREE_DEPTH] = [0; TREE_DEPTH];
    index1[0] = 1;
    (zeros, tip, f2, path0, path1, index0, index1)
}

fn append_two(
    frontier: [Field; TREE_DEPTH],
    zeros: [Field; TREE_DEPTH],
    at: u32,
    a: Field,
    b: Field,
) -> Field {
    let (f1, _) = insert(a, at, frontier, zeros);
    let (_f2, new_root) = insert(b, at + 1, f1, zeros);
    new_root
}

#[test]
fn transfer_valid() {
    let asset: Field = 7777;
    let sk: Field = 42;
    let nk = derive_nk(sk);
    let ivk = derive_ivk(sk);
    let addr = derive_addr(ivk, 7);
    let cm0 = note_commit(100, asset, addr, 11, 13);
    let cm1 = note_commit(50, asset, addr, 17, 19);
    let (zeros, tip, frontier, p0, p1, i0, i1) = two_leaf(cm0, cm1);
    let nf0 = nullifier(nk, cm0, 0);
    let nf1 = nullifier(nk, cm1, 1);
    let cmo0 = note_commit(120, asset, 999, 21, 23);
    let cmo1 = note_commit(25, asset, 888, 27, 29);
    let new_root = append_two(frontier, zeros, 2, cmo0, cmo1);
    main(
        sk, [100, 50], [7, 7], [11, 17], [13, 19], [p0, p1], [i0, i1], [1, 1],
        [120, 25], [999, 888], [21, 27], [23, 29], frontier,
        tip, asset, [nf0, nf1], [cmo0, cmo1], tip, new_root, 2, 0, 0, 5,
    );
}

#[test]
fn deposit_valid() {
    let asset: Field = 7777;
    let sk: Field = 42;
    let nk = derive_nk(sk);
    let ivk = derive_ivk(sk);
    let addr = derive_addr(ivk, 7);
    let dcm0 = note_commit(0, asset, addr, 101, 103);
    let dcm1 = note_commit(0, asset, addr, 107, 109);
    let nf0 = nullifier(nk, dcm0, 0);
    let nf1 = nullifier(nk, dcm1, 0);
    let cmo0 = note_commit(150, asset, 999, 21, 23);
    let cmo1 = note_commit(0, asset, 888, 27, 29);
    let z: [Field; TREE_DEPTH] = [0; TREE_DEPTH];
    let zeros = zeros_array();
    let empty: [Field; TREE_DEPTH] = [0; TREE_DEPTH];
    let old_root = root_with_leaf(ZERO, 0, empty, zeros);
    let new_root = append_two(empty, zeros, 0, cmo0, cmo1);
    main(
        sk, [0, 0], [7, 7], [101, 107], [103, 109], [z, z], [z, z], [0, 0],
        [150, 0], [999, 888], [21, 27], [23, 29], empty,
        0, asset, [nf0, nf1], [cmo0, cmo1], old_root, new_root, 0, 150, 0, 0,
    );
}

#[test]
fn withdraw_valid() {
    let asset: Field = 7777;
    let sk: Field = 42;
    let nk = derive_nk(sk);
    let ivk = derive_ivk(sk);
    let addr = derive_addr(ivk, 7);
    let cm0 = note_commit(100, asset, addr, 11, 13);
    let cm1 = note_commit(50, asset, addr, 17, 19);
    let (zeros, tip, frontier, p0, p1, i0, i1) = two_leaf(cm0, cm1);
    let nf0 = nullifier(nk, cm0, 0);
    let nf1 = nullifier(nk, cm1, 1);
    let cmo0 = note_commit(10, asset, addr, 21, 23);
    let cmo1 = note_commit(0, asset, 888, 27, 29);
    let new_root = append_two(frontier, zeros, 2, cmo0, cmo1);
    main(
        sk, [100, 50], [7, 7], [11, 17], [13, 19], [p0, p1], [i0, i1], [1, 1],
        [10, 0], [addr, 888], [21, 27], [23, 29], frontier,
        tip, asset, [nf0, nf1], [cmo0, cmo1], tip, new_root, 2, 0, 140, 0,
    );
}

#[test(should_fail)]
fn unbalanced_fails() {
    let asset: Field = 7777;
    let sk: Field = 42;
    let nk = derive_nk(sk);
    let ivk = derive_ivk(sk);
    let addr = derive_addr(ivk, 7);
    let cm0 = note_commit(100, asset, addr, 11, 13);
    let cm1 = note_commit(50, asset, addr, 17, 19);
    let (zeros, tip, frontier, p0, p1, i0, i1) = two_leaf(cm0, cm1);
    let nf0 = nullifier(nk, cm0, 0);
    let nf1 = nullifier(nk, cm1, 1);
    let cmo0 = note_commit(120, asset, 999, 21, 23);
    let cmo1 = note_commit(25, asset, 888, 27, 29);
    let new_root = append_two(frontier, zeros, 2, cmo0, cmo1);
    main(
        sk, [100, 50], [7, 7], [11, 17], [13, 19], [p0, p1], [i0, i1], [1, 1],
        [120, 25], [999, 888], [21, 27], [23, 29], frontier,
        tip, asset, [nf0, nf1], [cmo0, cmo1], tip, new_root, 2, 0, 0, 4,
    );
}

#[test(should_fail)]
fn bad_new_root_fails() {
    let asset: Field = 7777;
    let sk: Field = 42;
    let nk = derive_nk(sk);
    let ivk = derive_ivk(sk);
    let addr = derive_addr(ivk, 7);
    let cm0 = note_commit(100, asset, addr, 11, 13);
    let cm1 = note_commit(50, asset, addr, 17, 19);
    let (_zeros, tip, frontier, p0, p1, i0, i1) = two_leaf(cm0, cm1);
    let nf0 = nullifier(nk, cm0, 0);
    let nf1 = nullifier(nk, cm1, 1);
    let cmo0 = note_commit(120, asset, 999, 21, 23);
    let cmo1 = note_commit(25, asset, 888, 27, 29);
    main(
        sk, [100, 50], [7, 7], [11, 17], [13, 19], [p0, p1], [i0, i1], [1, 1],
        [120, 25], [999, 888], [21, 27], [23, 29], frontier,
        tip, asset, [nf0, nf1], [cmo0, cmo1], tip, 999999, 2, 0, 0, 5,
    );
}

#[test]
fn print_deposit_witness() {
    let asset: Field = 246862576160547569215256475045468302459;
    let sk: Field = 42;
    let nk = derive_nk(sk);
    let ivk = derive_ivk(sk);
    let addr = derive_addr(ivk, 7);
    let dcm0 = note_commit(0, asset, addr, 101, 103);
    let dcm1 = note_commit(0, asset, addr, 107, 109);
    let zeros = zeros_array();
    let empty: [Field; TREE_DEPTH] = [0; TREE_DEPTH];
    let cmo0 = note_commit(150, asset, 999, 21, 23);
    let cmo1 = note_commit(0, asset, 888, 27, 29);
    println("nf0"); println(nullifier(nk, dcm0, 0));
    println("nf1"); println(nullifier(nk, dcm1, 0));
    println("cmo0"); println(cmo0);
    println("cmo1"); println(cmo1);
    println("old_root"); println(root_with_leaf(ZERO, 0, empty, zeros));
    println("new_root"); println(append_two(empty, zeros, 0, cmo0, cmo1));
    println("addr"); println(addr);
}

// Membership path + root for a leaf among the first 8 positions (dense height-3
// subtree, then zeros to depth 20). Consistent with the incremental `insert`
// root for the same leaves (empty slots = 0).
fn path8(leaves: [Field; 8], p: u32, zeros: [Field; TREE_DEPTH]) -> ([Field; TREE_DEPTH], [Field; TREE_DEPTH], Field) {
    let l0 = leaves;
    let mut l1: [Field; 4] = [0; 4];
    for i in 0..4 { l1[i] = hash2(l0[2*i], l0[2*i+1]); }
    let mut l2: [Field; 2] = [0; 2];
    for i in 0..2 { l2[i] = hash2(l1[2*i], l1[2*i+1]); }
    let sub = hash2(l2[0], l2[1]);
    let mut root = sub;
    for i in 3..TREE_DEPTH { root = hash2(root, zeros[i]); }
    let mut path: [Field; TREE_DEPTH] = [0; TREE_DEPTH];
    let mut idx: [Field; TREE_DEPTH] = [0; TREE_DEPTH];
    path[0] = if p & 1 == 0 { l0[p + 1] } else { l0[p - 1] };
    let p1 = p >> 1;
    path[1] = if p1 & 1 == 0 { l1[p1 + 1] } else { l1[p1 - 1] };
    let p2 = p >> 2;
    path[2] = if p2 & 1 == 0 { l2[p2 + 1] } else { l2[p2 - 1] };
    for i in 3..TREE_DEPTH { path[i] = zeros[i]; }
    idx[0] = (p & 1) as Field;
    idx[1] = ((p >> 1) & 1) as Field;
    idx[2] = ((p >> 2) & 1) as Field;
    (path, idx, root)
}

// Simulate deposit -> transfer -> withdraw and print every witness value.
#[test]
fn print_chain() {
    let asset: Field = 246862576160547569215256475045468302459;
    // Alice
    let ska: Field = 42; let nka = derive_nk(ska); let iva = derive_ivk(ska);
    let addra = derive_addr(iva, 7);
    // Bob
    let skb: Field = 99; let nkb = derive_nk(skb); let ivb = derive_ivk(skb);
    let addrb = derive_addr(ivb, 5);
    let zeros = zeros_array();
    let empty: [Field; TREE_DEPTH] = [0; TREE_DEPTH];
    std::println("ADDRA"); std::println(addra);
    std::println("ADDRB"); std::println(addrb);

    // ---- DEPOSIT: 150 in, output noteA(150->Alice) + dummyOut(0) ----
    let cm_a = note_commit(150, asset, addra, 11, 13);
    let dout = note_commit(0, asset, 888, 27, 29);
    let ddep0 = note_commit(0, asset, addra, 101, 103);
    let ddep1 = note_commit(0, asset, addra, 107, 109);
    let (f_after_dep, root1) = {
        let (f1, _) = insert(cm_a, 0, empty, zeros);
        insert(dout, 1, f1, zeros)
    };
    std::println("DEP nf0"); std::println(nullifier(nka, ddep0, 0));
    std::println("DEP nf1"); std::println(nullifier(nka, ddep1, 0));
    std::println("DEP cm0"); std::println(cm_a);
    std::println("DEP cm1"); std::println(dout);
    std::println("DEP new_root"); std::println(root1);

    // ---- TRANSFER: Alice spends noteA@0, outputs noteBob(100) + change(50) ----
    let mut leaves: [Field; 8] = [0; 8];
    leaves[0] = cm_a; leaves[1] = dout;
    let (pa, ia, _r) = path8(leaves, 0, zeros); // path of noteA@0 vs root1
    let cm_bob = note_commit(100, asset, addrb, 31, 33);
    let cm_chg = note_commit(50, asset, addra, 41, 43);
    let tdummy = note_commit(0, asset, addra, 201, 203);
    let (f_after_xfer, root2) = {
        let (f1, _) = insert(cm_bob, 2, f_after_dep, zeros);
        insert(cm_chg, 3, f1, zeros)
    };
    std::println("XFER path0"); std::println(pa);
    std::println("XFER index0"); std::println(ia);
    std::println("XFER nf0"); std::println(nullifier(nka, cm_a, 0));
    std::println("XFER nf1"); std::println(nullifier(nka, tdummy, 0));
    std::println("XFER cm0"); std::println(cm_bob);
    std::println("XFER cm1"); std::println(cm_chg);
    std::println("XFER filled"); std::println(f_after_dep);
    std::println("XFER old_root(root1)"); std::println(root1);
    std::println("XFER new_root(root2)"); std::println(root2);

    // ---- WITHDRAW: Bob spends noteBob@2, vpub_out=100 ----
    leaves[2] = cm_bob; leaves[3] = cm_chg;
    let (pb, ib, _r2) = path8(leaves, 2, zeros); // path of noteBob@2 vs root2
    let wd0 = note_commit(0, asset, 888, 51, 53);
    let wd1 = note_commit(0, asset, 888, 61, 63);
    let wdummy = note_commit(0, asset, addrb, 301, 303);
    let (_f, root3) = {
        let (f1, _) = insert(wd0, 4, f_after_xfer, zeros);
        insert(wd1, 5, f1, zeros)
    };
    std::println("WD path0"); std::println(pb);
    std::println("WD index0"); std::println(ib);
    std::println("WD nf0"); std::println(nullifier(nkb, cm_bob, 2));
    std::println("WD nf1"); std::println(nullifier(nkb, wdummy, 0));
    std::println("WD cm0"); std::println(wd0);
    std::println("WD cm1"); std::println(wd1);
    std::println("WD filled"); std::println(f_after_xfer);
    std::println("WD old_root(root2)"); std::println(root2);
    std::println("WD new_root(root3)"); std::println(root3);
}
