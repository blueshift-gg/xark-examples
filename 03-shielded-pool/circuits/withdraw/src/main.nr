// Shielded pool — WITHDRAW circuit. Proves membership of your commitment in a
// remembered `root` without revealing which leaf, and reveals `nullifier_hash`
// so the pool can mark the note spent. recipient/relayer/fee are public inputs,
// so Groth16 binds them to the proof — nobody can re-target the withdrawal.
// Pubkeys exceed the BN254 field, so each is split into two 128-bit halves
// (hi/lo); the program splits them identically. Public inputs, in order: root,
// nullifier_hash, recipient_hi, recipient_lo, relayer_hi, relayer_lo, fee.
// Private: secret, nullifier, path_elements, path_indices. (See the README.)

global H: u32 = 20;

fn main(
    secret: Field,
    nullifier: Field,
    path_elements: [Field; H],
    path_indices: [Field; H], // each 0 (current node is left child) or 1 (right child)
    root: pub Field,
    nullifier_hash: pub Field,
    recipient_hi: pub Field,
    recipient_lo: pub Field,
    relayer_hi: pub Field,
    relayer_lo: pub Field,
    fee: pub Field,
) {
    // 1. Recompute the commitment from its secret parts.
    let commitment = hash2(nullifier, secret);

    // 2. Prove the commitment is a leaf in the tree rooted at `root`.
    let mut current = commitment;
    for i in 0..H {
        let bit = path_indices[i];
        assert(bit * (bit - 1) == 0); // constrain bit ∈ {0, 1}
        let left = if bit == 1 { path_elements[i] } else { current };
        let right = if bit == 1 { current } else { path_elements[i] };
        current = hash2(left, right);
    }
    assert(current == root);

    // 3. Reveal the nullifier hash (the pool marks it spent to prevent re-use).
    assert(poseidon1(nullifier) == nullifier_hash);

    // 4. Binding to a specific recipient/relayer comes from Groth16 over the
    //    hi/lo halves the program derives from real pubkeys. We range-check each
    //    half (16-byte, < 2^128) and fee (< 2^64) — which also matches the
    //    program's split and keeps the inputs in the public-input vector.
    assert(recipient_hi as u128 as Field == recipient_hi);
    assert(recipient_lo as u128 as Field == recipient_lo);
    assert(relayer_hi as u128 as Field == relayer_hi);
    assert(relayer_lo as u128 as Field == relayer_lo);
    assert(fee as u64 as Field == fee);
}

fn hash2(l: Field, r: Field) -> Field {
    std::hash::poseidon2_permutation([l, r, 0, 0])[0]
}

// Domain-separated single-input hash for the nullifier. The `1` sits in a
// capacity lane that hash2 always leaves 0, so a nullifier hash can never equal
// a commitment/tree hash (which would otherwise happen for secret = 0).
fn poseidon1(x: Field) -> Field {
    std::hash::poseidon2_permutation([x, 0, 1, 0])[0]
}

// Build the single-leaf (index 0) tree for `commitment`: every sibling is the
// empty-subtree root zeros[i], and leaf 0 is always the left child.
fn single_leaf_path_and_root(commitment: Field) -> ([Field; H], Field) {
    let mut path: [Field; H] = [0; H];
    let mut cur = commitment;
    let mut zi: Field = 0;
    for i in 0..H {
        path[i] = zi;
        cur = hash2(cur, zi);
        zi = hash2(zi, zi);
    }
    (path, cur)
}

#[test]
fn withdraw_valid() {
    let (secret, nullifier) = (1, 2);
    let (path, root) = single_leaf_path_and_root(hash2(nullifier, secret));
    main(secret, nullifier, path, [0; H], root, poseidon1(nullifier), 0, 0, 0, 0, 0);
}

#[test(should_fail)]
fn withdraw_non_member() {
    let (secret, nullifier) = (1, 2);
    let (mut path, root) = single_leaf_path_and_root(hash2(nullifier, secret));
    path[0] = 42; // wrong sibling → recomputed root ≠ root
    main(secret, nullifier, path, [0; H], root, poseidon1(nullifier), 0, 0, 0, 0, 0);
}

#[test(should_fail)]
fn withdraw_wrong_nullifier_hash() {
    let (secret, nullifier) = (1, 2);
    let (path, root) = single_leaf_path_and_root(hash2(nullifier, secret));
    main(secret, nullifier, path, [0; H], root, poseidon1(nullifier) + 1, 0, 0, 0, 0, 0);
}

#[test(should_fail)]
fn withdraw_non_boolean_index() {
    let (secret, nullifier) = (1, 2);
    let (path, root) = single_leaf_path_and_root(hash2(nullifier, secret));
    let mut idx: [Field; H] = [0; H];
    idx[0] = 2; // violates bit ∈ {0,1}
    main(secret, nullifier, path, idx, root, poseidon1(nullifier), 0, 0, 0, 0, 0);
}
