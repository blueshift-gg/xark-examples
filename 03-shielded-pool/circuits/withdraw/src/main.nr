// Shielded pool — WITHDRAW circuit.
//
// Prove, without revealing WHICH deposit is yours:
//   1. you know the (secret, nullifier) behind a commitment that sits in the
//      tree rooted at `root` (a recent root the pool remembers), and
//   2. `nullifier_hash` is the hash of that nullifier — revealed once, so the
//      pool can mark it spent and stop double-withdrawals.
//
// `recipient`, `relayer`, and `fee` are public inputs. In Groth16 every public
// input is bound to the proof (the verifier folds them into `vk_x`), so a
// proof made for one recipient can't be replayed to pay a different one — this
// is what stops a relayer or front-runner from stealing the withdrawal.
//
// Public inputs (IN THIS ORDER — the program builds them identically):
//   root, nullifier_hash, recipient_hi, recipient_lo, relayer_hi, relayer_lo, fee
// Private inputs:
//   secret, nullifier, path_elements, path_indices
//
// A Solana pubkey is 256-bit and can exceed the BN254 field, so recipient and
// relayer are each split into two 128-bit halves (hi/lo). The program splits
// them the same way. Verify the ordering against `xark inspect`.

use std::hash::poseidon2::Poseidon2;

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

    // 4. Bind the transaction parameters. They're public inputs, so Groth16
    //    already ties them to the proof; we reference them so nargo keeps them
    //    in the ABI and emits no unused-input warning.
    let _bound = (recipient_hi, recipient_lo, relayer_hi, relayer_lo, fee);
}

fn hash2(l: Field, r: Field) -> Field {
    Poseidon2::hash([l, r], 2)
}

fn poseidon1(x: Field) -> Field {
    Poseidon2::hash([x], 1)
}
