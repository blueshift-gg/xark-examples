// Circuit tests + witness generation for the `transact` circuit.
//
// This module is separate from the circuit itself (src/main.nr) so that file
// reads as just the statement being proved. Two kinds of thing live here:
//   - `#[test]` / `#[test(should_fail)]` correctness checks that call `main`
//     directly with a fully-worked witness;
//   - `print_*` helpers that simulate an evolving tree and `println` every
//     witness value — `scripts/gen_chain.py` runs `nargo test print_chain`,
//     parses the output, and assembles the three Prover.toml files.
//
// Everything the tests need is imported from the crate root; `main` is a
// pub-crate entrypoint so these tests can drive it.

use crate::{
    derive_addr, derive_ivk, derive_nk, hash2, insert, main, note_commit, nullifier,
    root_with_leaf, zeros_array, TREE_DEPTH, ZERO,
};

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
