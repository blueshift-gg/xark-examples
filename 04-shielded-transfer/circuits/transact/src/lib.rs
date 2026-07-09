//! Shielded transfer — JoinSplit-style `transact` circuit (2-in / 2-out).
//!
//! One primitive covers deposit, private transfer, and withdraw via public
//! value slots: `vpub_in` (public value entering the shield, e.g. an SPL
//! deposit) and `vpub_out` (public value leaving, e.g. a withdraw).
//! Fully-shielded transfers set both to 0. Inputs can be "dummy" (value 0,
//! membership skipped) so a deposit needs no real inputs.
//!
//! The circuit proves, in one shot:
//!   - spend side: input notes are real leaves (membership vs the anchor
//!     `root`), only the owner can spend (key derivation), each burns a
//!     position-bound nullifier;
//!   - output side: outputs are well-formed commitments AND are correctly
//!     appended to the current tree (`old_root` → `new_root` via a private
//!     frontier) — 03's trick. The two leaves land as one pair: `insert_index`
//!     is always even (the program appends 2 per transact from 0), so
//!     `hash2(cm0, cm1)` folds in from level 1, halving the membership work;
//!   - value is conserved: `sum_in + vpub_in == sum_out + vpub_out + fee`.
//!
//! Poseidon2 domains: every object hashes with a distinct sponge arity/tag
//! (see `hash::<N>`'s length-seeded capacity) plus an explicit domain constant,
//! so keys, addresses, commitments, nullifiers and tree nodes can never
//! collide across kinds.
//!
//! Note *delivery* (the encrypted memo envelope) is off-circuit: the sender
//! publishes it as event data and the recipient trial-decrypts. SHA-256 hashes
//! of both envelopes are public inputs, so a submitter cannot replace them
//! without invalidating the proof.
//!
//! Public inputs, in order:
//!   root, asset, nf[0], nf[1], cm_out[0], cm_out[1],
//!   old_root, new_root, insert_index, vpub_in, vpub_out, fee,
//!   recipient_hi, recipient_lo,
//!   memo0_hash_hi, memo0_hash_lo, memo1_hash_hi, memo1_hash_lo
#![no_std]

use pool_merkle::{merkle_root, mux, zeros, H};
use xark::prelude::*;
use xark_poseidon2::{hash, hash2};

const NINS: usize = 2;
const NOUTS: usize = 2;

// Domain tags (belt-and-braces on top of the sponge's length tag).
const D_NK: u64 = 1;
const D_IVK: u64 = 2;
const D_ADDR: u64 = 3;
const D_CM: u64 = 4;
const D_NF: u64 = 5;

fn derive_nk(sk: Field) -> Field {
    hash::<2>([sk, Field::from(D_NK)])
}
fn derive_ivk(sk: Field) -> Field {
    hash::<2>([sk, Field::from(D_IVK)])
}
fn derive_addr(ivk: Field, d: Field) -> Field {
    hash::<3>([ivk, d, Field::from(D_ADDR)])
}
fn note_commit(value: Field, asset: Field, addr: Field, rho: Field, rseed: Field) -> Field {
    hash::<6>([value, asset, addr, rho, rseed, Field::from(D_CM)])
}
fn nullifier(nk: Field, cm: Field, pos: Field) -> Field {
    hash::<4>([nk, cm, pos, Field::from(D_NF)])
}

/// Fold a level-1 node up to the root: `bits`/`filled`/`zeros` indexed by
/// height, entries `1..H` used (an even-aligned pair occupies one level-1 slot).
fn root_from_level1(node: Field, bits: [Field; H], filled: [Field; H], z: [Field; H]) -> Field {
    let mut current = node;
    let mut i = 1usize;
    while i < H {
        let b = bits[i];
        let l = mux(b, filled[i], current);
        let r = mux(b, current, z[i]);
        current = hash2(l, r);
        i += 1;
    }
    current
}

// Keep explicit assignments in the circuit subset instead of relying on
// AddAssign lowering support.
#[allow(clippy::assign_op_pattern, clippy::too_many_arguments)]
pub fn circuit(
    sk: Private<Field>,
    in_value: Private<[Field; NINS]>,
    in_d: Private<[Field; NINS]>,
    in_rho: Private<[Field; NINS]>,
    in_rseed: Private<[Field; NINS]>,
    in_path: Private<[[Field; H]; NINS]>,
    in_bits: Private<[[Field; H]; NINS]>, // auth-path direction bits, LSB-first
    in_enforce: Private<[Field; NINS]>,   // 1 = real spend, 0 = dummy
    out_value: Private<[Field; NOUTS]>,
    out_addr: Private<[Field; NOUTS]>,
    out_rho: Private<[Field; NOUTS]>,
    out_rseed: Private<[Field; NOUTS]>,
    filled_subtrees: Private<[Field; H]>, // frontier at insert_index ([0] unused: pair-aligned)
    root: Public<Field>,
    asset: Public<Field>,
    nf: Public<[Field; NINS]>,
    cm_out: Public<[Field; NOUTS]>,
    old_root: Public<Field>,
    new_root: Public<Field>,
    insert_index: Public<Field>,
    vpub_in: Public<Field>,
    vpub_out: Public<Field>,
    fee: Public<Field>,
    recipient_hi: Public<Field>,
    recipient_lo: Public<Field>,
    memo0_hash_hi: Public<Field>,
    memo0_hash_lo: Public<Field>,
    memo1_hash_hi: Public<Field>,
    memo1_hash_lo: Public<Field>,
) {
    let nk = derive_nk(sk);
    let ivk = derive_ivk(sk);

    // ---- spend side ----
    let mut sum_in = Field::from(0u64);
    let mut i = 0usize;
    while i < NINS {
        let e = in_enforce[i];
        assert_eq(e * e, e); // e ∈ {0, 1}
        let _ = in_value[i].to_bits::<64>(); // value < 2^64
        assert_eq(in_value[i] * (Field::from(1u64) - e), Field::from(0u64)); // dummies carry 0

        let addr = derive_addr(ivk, in_d[i]);
        let cm = note_commit(in_value[i], asset, addr, in_rho[i], in_rseed[i]);

        // Position from the path bits (booleanity asserted in merkle_root).
        let mut pos = Field::from(0u64);
        let mut pow = Field::from(1u64);
        let mut k = 0usize;
        while k < H {
            pos = pos + in_bits[i][k] * pow;
            pow = pow + pow;
            k += 1;
        }

        // Membership only for real inputs; nullifiers burn either way.
        let computed = merkle_root(cm, in_path[i], in_bits[i]);
        assert_eq((computed - root) * e, Field::from(0u64));
        assert_eq(nullifier(nk, cm, pos), nf[i]);
        sum_in = sum_in + in_value[i];
        i += 1;
    }

    // ---- output side: well-formed commitments ----
    let mut sum_out = Field::from(0u64);
    let mut j = 0usize;
    while j < NOUTS {
        let _ = out_value[j].to_bits::<64>();
        let cm = note_commit(out_value[j], asset, out_addr[j], out_rho[j], out_rseed[j]);
        assert_eq(cm, cm_out[j]);
        sum_out = sum_out + out_value[j];
        j += 1;
    }

    // ---- append the pair: old_root → new_root ----
    // `to_bits` range-checks insert_index < 2^H; bit 0 must be 0 (pair-aligned).
    let bits = insert_index.to_bits::<H>();
    assert_eq(bits[0], Field::from(0u64));
    let z = zeros();
    // Bind the private frontier to the current tip (the pair's slot is empty)…
    assert_eq(root_from_level1(z[1], bits, filled_subtrees, z), old_root);
    // …then the same tree with the pair present is the new root.
    let pair = hash2(cm_out[0], cm_out[1]);
    assert_eq(root_from_level1(pair, bits, filled_subtrees, z), new_root);

    // ---- value conservation ----
    let _ = vpub_in.to_bits::<64>();
    let _ = vpub_out.to_bits::<64>();
    let _ = fee.to_bits::<64>();
    assert_eq(sum_in + vpub_in, sum_out + vpub_out + fee);

    // Bind the withdrawal destination and encrypted note-delivery envelopes.
    // Each 32-byte value is represented as two canonical 128-bit limbs.
    let _ = recipient_hi.to_bits::<128>();
    let _ = recipient_lo.to_bits::<128>();
    let _ = memo0_hash_hi.to_bits::<128>();
    let _ = memo0_hash_lo.to_bits::<128>();
    let _ = memo1_hash_hi.to_bits::<128>();
    let _ = memo1_hash_lo.to_bits::<128>();
}
