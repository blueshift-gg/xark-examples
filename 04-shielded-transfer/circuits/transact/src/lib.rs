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
use pool_merkle::{H, merkle_root, mux, zeros};
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

/// Everything known only to the prover. `CircuitInput` derives the exact
/// host-to-circuit leaf mapping for this nested witness in declaration order.
#[derive(Clone, Copy, Debug, CircuitInput)]
pub struct TransactWitness {
    pub sk: Field,
    pub in_value: [Field; NINS],
    pub in_d: [Field; NINS],
    pub in_rho: [Field; NINS],
    pub in_rseed: [Field; NINS],
    pub in_path: [[Field; H]; NINS],
    pub in_bits: [[Field; H]; NINS],
    pub in_enforce: [Field; NINS],
    pub out_value: [Field; NOUTS],
    pub out_addr: [Field; NOUTS],
    pub out_rho: [Field; NOUTS],
    pub out_rseed: [Field; NOUTS],
    pub filled_subtrees: [Field; H],
}

/// The verifier-visible statement. Field order is the on-chain public-input
/// ABI; keep it in lockstep with the program and E2E instruction builder.
#[derive(Clone, Copy, Debug, CircuitInput)]
pub struct TransactStatement {
    pub root: Field,
    pub asset: Field,
    pub nf: [Field; NINS],
    pub cm_out: [Field; NOUTS],
    pub old_root: Field,
    pub new_root: Field,
    pub insert_index: Field,
    pub vpub_in: Field,
    pub vpub_out: Field,
    pub fee: Field,
    pub recipient_hi: Field,
    pub recipient_lo: Field,
    pub memo0_hash_hi: Field,
    pub memo0_hash_lo: Field,
    pub memo1_hash_hi: Field,
    pub memo1_hash_lo: Field,
}

#[circuit]
pub fn shielded_transact(witness: Private<TransactWitness>, statement: Public<TransactStatement>) {
    let nk = derive_nk(witness.sk);
    let ivk = derive_ivk(witness.sk);

    // ---- spend side ----
    let mut sum_in = Field::from(0u64);
    let mut i = 0usize;
    while i < NINS {
        let e = witness.in_enforce[i];
        require_eq(e * e, e); // e is boolean
        let _ = witness.in_value[i].to_bits::<64>(); // value < 2^64
        require_eq(
            witness.in_value[i] * (Field::from(1u64) - e),
            Field::from(0u64),
        ); // dummies carry 0

        let addr = derive_addr(ivk, witness.in_d[i]);
        let cm = note_commit(
            witness.in_value[i],
            statement.asset,
            addr,
            witness.in_rho[i],
            witness.in_rseed[i],
        );

        // Position from the path bits (booleanity asserted in merkle_root).
        let mut pos = Field::from(0u64);
        let mut pow = Field::from(1u64);
        let mut k = 0usize;
        while k < H {
            pos = pos + witness.in_bits[i][k] * pow;
            pow = pow + pow;
            k += 1;
        }

        // Membership only for real inputs; nullifiers burn either way.
        let computed = merkle_root(cm, witness.in_path[i], witness.in_bits[i]);
        require_eq((computed - statement.root) * e, Field::from(0u64));
        require_eq(nullifier(nk, cm, pos), statement.nf[i]);
        sum_in = sum_in + witness.in_value[i];
        i += 1;
    }

    // ---- output side: well-formed commitments ----
    let mut sum_out = Field::from(0u64);
    let mut j = 0usize;
    while j < NOUTS {
        let _ = witness.out_value[j].to_bits::<64>();
        let cm = note_commit(
            witness.out_value[j],
            statement.asset,
            witness.out_addr[j],
            witness.out_rho[j],
            witness.out_rseed[j],
        );
        require_eq(cm, statement.cm_out[j]);
        sum_out = sum_out + witness.out_value[j];
        j += 1;
    }

    // ---- append the pair: old_root → new_root ----
    // `to_bits` range-checks insert_index < 2^H; bit 0 must be 0 (pair-aligned).
    let bits = statement.insert_index.to_bits::<H>();
    require_eq(bits[0], Field::from(0u64));
    let z = zeros();
    // Bind the private frontier to the current tip (the pair's slot is empty)…
    require_eq(
        root_from_level1(z[1], bits, witness.filled_subtrees, z),
        statement.old_root,
    );
    // …then the same tree with the pair present is the new root.
    let pair = hash2(statement.cm_out[0], statement.cm_out[1]);
    require_eq(
        root_from_level1(pair, bits, witness.filled_subtrees, z),
        statement.new_root,
    );

    // ---- value conservation ----
    let _ = statement.vpub_in.to_bits::<64>();
    let _ = statement.vpub_out.to_bits::<64>();
    let _ = statement.fee.to_bits::<64>();
    require_eq(
        sum_in + statement.vpub_in,
        sum_out + statement.vpub_out + statement.fee,
    );

    // Bind the withdrawal destination and encrypted note-delivery envelopes.
    // Each 32-byte value is represented as two canonical 128-bit limbs.
    let _ = statement.recipient_hi.to_bits::<128>();
    let _ = statement.recipient_lo.to_bits::<128>();
    let _ = statement.memo0_hash_hi.to_bits::<128>();
    let _ = statement.memo0_hash_lo.to_bits::<128>();
    let _ = statement.memo1_hash_hi.to_bits::<128>();
    let _ = statement.memo1_hash_lo.to_bits::<128>();
}
