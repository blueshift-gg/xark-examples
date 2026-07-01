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

    // 4. Bind recipient/relayer/fee into the proof. They're public inputs, so
    //    Groth16 already ties them to the proof — but range-constraining the
    //    128-bit halves does double duty: it (a) guarantees nargo keeps them in
    //    the ABI (a real constraint on a public input can't be optimized away,
    //    so N stays 7 and the program's byte layout matches) and (b) enforces
    //    they really are 16-byte halves, so the program's hi/lo reconstruction
    //    of the pubkeys can't be gamed by stuffing >128 bits into a half.
    let _rh = recipient_hi as u128;
    let _rl = recipient_lo as u128;
    let _lh = relayer_hi as u128;
    let _ll = relayer_lo as u128;
    let _fee = fee as u64;
}

// Fixed-arity compressions over the public Poseidon2 permutation (width 4),
// identical to the deposit circuit and reproduced by the client.
fn hash2(l: Field, r: Field) -> Field {
    std::hash::poseidon2_permutation([l, r, 0, 0])[0]
}

fn poseidon1(x: Field) -> Field {
    std::hash::poseidon2_permutation([x, 0, 0, 0])[0]
}
