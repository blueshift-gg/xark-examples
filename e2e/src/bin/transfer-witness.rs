//! Witness generator for the shielded transfer (example 04) — the wallet side.
//!
//!     cargo run --bin transfer-witness -- dep    # Alice shields 150
//!     cargo run --bin transfer-witness -- xfer   # Alice pays Bob 100 (50 change)
//!     cargo run --bin transfer-witness -- wd     # Bob unshields 100
//!     cargo run --bin transfer-witness -- empty-root
//!
//! Replays the 3-transaction demo chain natively (same Poseidon2, same note
//! algebra as the circuit) and prints a flat input-file document for the
//! requested transaction.

use ark_bn254::Fr;
use ark_ff::{PrimeField, Zero};
use sha2::{Digest, Sha256};
use xark_examples_e2e::{
    fixtures, fr_to_decimal, fr_to_le,
    merkle::{IncrementalTree, H},
    notes::{derive_addr, derive_ivk, derive_nk, note_commit, nullifier},
    split_pubkey,
};

/// One side of a JoinSplit as a flat `xark prove --input-file` document.
struct Args(Vec<String>);

impl Args {
    fn new() -> Self {
        Args(Vec::new())
    }
    fn one(&mut self, name: &str, v: Fr) {
        self.0.push(format!("{name} = {}", fr_to_decimal(v)));
    }
    fn many(&mut self, name: &str, vs: &[Fr]) {
        for (i, v) in vs.iter().enumerate() {
            self.one(&format!("{name}[{i}]"), *v);
        }
    }
    fn grid(&mut self, name: &str, vs: &[[Fr; H]]) {
        for (i, row) in vs.iter().enumerate() {
            for (k, v) in row.iter().enumerate() {
                self.one(&format!("{name}[{i}][{k}]"), *v);
            }
        }
    }
}

/// A note in flight: enough to spend it later.
#[derive(Clone, Copy)]
struct Note {
    value: u64,
    d: u64,
    rho: u64,
    rseed: u64,
    index: u32,
}

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_default();

    // The circuit's asset tag: low 16 bytes of the mint pubkey.
    let asset = Fr::from_le_bytes_mod_order(&fixtures::MINT_PUBKEY[..16]);

    let sk_a = Fr::from(42u64);
    let sk_b = Fr::from(99u64);
    let addr_a = derive_addr(derive_ivk(sk_a), Fr::from(7u64));
    let addr_b = derive_addr(derive_ivk(sk_b), Fr::from(5u64));
    let addr_burn = Fr::from(888u64); // dummy outputs go nowhere
    let (recipient_hi, recipient_lo) = split_pubkey(&fixtures::BOB_TOKEN_PUBKEY);

    let mut tree = IncrementalTree::new();

    if mode == "empty-root" {
        println!("const EMPTY_ROOT: [u8; 32] = {:?};", fr_to_le(tree.root()));
        return;
    }

    // ---- the demo chain, replayed deterministically ----
    // tx1 dep:  dummies in, [Alice 150, dummy 0] out, vpub_in = 150, index 0.
    // tx2 xfer: spend Alice's 150 note, [Bob 100, Alice change 50] out, index 2.
    // tx3 wd:   spend Bob's 100 note, dummy outs, vpub_out = 100, index 4.
    let alice_note = Note {
        value: 150,
        d: 7,
        rho: 11,
        rseed: 13,
        index: 0,
    };
    let bob_note = Note {
        value: 100,
        d: 5,
        rho: 31,
        rseed: 33,
        index: 2,
    };

    let emit = |a: Args| println!("{}", a.0.join("\n"));

    // Per-transaction closure: assemble one JoinSplit's inputs.
    #[allow(clippy::too_many_arguments)]
    let transact = |tree: &mut IncrementalTree,
                    sk: Fr,
                    ins: [(Option<Note>, u64, u64); 2], // (real note?, dummy rho, dummy rseed)
                    outs: [(u64, Fr, u64, u64); 2],     // (value, addr, rho, rseed)
                    vpub_in: u64,
                    vpub_out: u64,
                    memos: [&[u8]; 2]|
     -> Args {
        let ivk = derive_ivk(sk);
        let nk = derive_nk(sk);
        let root = tree.root();

        let mut a = Args::new();
        a.one("sk", sk);

        let (mut vals, mut ds, mut rhos, mut rseeds, mut enf) =
            (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let mut paths: Vec<[Fr; H]> = Vec::new();
        let mut bits_rows: Vec<[Fr; H]> = Vec::new();
        let mut nfs = Vec::new();

        for (real, drho, drseed) in ins {
            let (note, enforce) = match real {
                Some(n) => (n, 1u64),
                None => (
                    Note {
                        value: 0,
                        d: 7,
                        rho: drho,
                        rseed: drseed,
                        index: 0,
                    },
                    0u64,
                ),
            };
            let addr = derive_addr(ivk, Fr::from(note.d));
            let cm = note_commit(
                Fr::from(note.value),
                asset,
                addr,
                Fr::from(note.rho),
                Fr::from(note.rseed),
            );
            let (path, bits) = if enforce == 1 {
                tree.auth_path(note.index)
            } else {
                ([Fr::zero(); H], [0u8; H])
            };
            let pos: u32 = bits.iter().enumerate().map(|(k, b)| (*b as u32) << k).sum();
            nfs.push(nullifier(nk, cm, Fr::from(pos)));
            vals.push(Fr::from(note.value));
            ds.push(Fr::from(note.d));
            rhos.push(Fr::from(note.rho));
            rseeds.push(Fr::from(note.rseed));
            enf.push(Fr::from(enforce));
            paths.push(path);
            bits_rows.push(bits.map(|b| Fr::from(b as u64)));
        }

        a.many("in_value", &vals);
        a.many("in_d", &ds);
        a.many("in_rho", &rhos);
        a.many("in_rseed", &rseeds);
        a.grid("in_path", &paths);
        a.grid("in_bits", &bits_rows);
        a.many("in_enforce", &enf);

        let mut cms = Vec::new();
        let (mut ovals, mut oaddrs, mut orhos, mut orseeds) =
            (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for (value, addr, rho, rseed) in outs {
            cms.push(note_commit(
                Fr::from(value),
                asset,
                addr,
                Fr::from(rho),
                Fr::from(rseed),
            ));
            ovals.push(Fr::from(value));
            oaddrs.push(addr);
            orhos.push(Fr::from(rho));
            orseeds.push(Fr::from(rseed));
        }
        a.many("out_value", &ovals);
        a.many("out_addr", &oaddrs);
        a.many("out_rho", &orhos);
        a.many("out_rseed", &orseeds);

        let insert_index = tree.next_index;
        let old_root = tree.root();
        let frontier = tree.append(cms[0]);
        tree.append(cms[1]);
        let new_root = tree.root();

        a.many("filled_subtrees", &frontier);
        a.one("root", root);
        a.one("asset", asset);
        a.many("nf", &nfs);
        a.many("cm_out", &cms);
        a.one("old_root", old_root);
        a.one("new_root", new_root);
        a.one("insert_index", Fr::from(insert_index));
        a.one("vpub_in", Fr::from(vpub_in));
        a.one("vpub_out", Fr::from(vpub_out));
        a.one("fee", Fr::from(0u64));
        a.one("recipient_hi", recipient_hi);
        a.one("recipient_lo", recipient_lo);
        for (i, memo) in memos.iter().enumerate() {
            let digest: [u8; 32] = Sha256::digest(memo).into();
            let (hi, lo) = split_pubkey(&digest);
            a.one(&format!("memo{i}_hash_hi"), hi);
            a.one(&format!("memo{i}_hash_lo"), lo);
        }
        a
    };

    // Replay up to (and emit) the requested transaction.
    let dep = transact(
        &mut tree,
        sk_a,
        [(None, 101, 103), (None, 107, 109)],
        [(150, addr_a, 11, 13), (0, addr_burn, 27, 29)],
        150,
        0,
        [b"dep-memo0", b"dep-memo1"],
    );
    if mode == "dep" {
        return emit(dep);
    }

    let xfer = transact(
        &mut tree,
        sk_a,
        [(Some(alice_note), 0, 0), (None, 201, 203)],
        [(100, addr_b, 31, 33), (50, addr_a, 41, 43)],
        0,
        0,
        [b"xfer-memo0", b"xfer-memo1"],
    );
    if mode == "xfer" {
        return emit(xfer);
    }

    let wd = transact(
        &mut tree,
        sk_b,
        [(Some(bob_note), 0, 0), (None, 301, 303)],
        [(0, addr_burn, 51, 53), (0, addr_burn, 61, 63)],
        0,
        100,
        [b"wd-memo0", b"wd-memo1"],
    );
    if mode == "wd" {
        return emit(wd);
    }

    eprintln!("usage: transfer-witness <dep|xfer|wd|empty-root>");
    std::process::exit(2);
}
