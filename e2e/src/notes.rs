//! Native mirror of the transact circuit's note algebra (example 04): key
//! derivation, addresses, note commitments, nullifiers. Same Poseidon2 sponge,
//! same domain tags — a proof stops verifying the moment these drift.

use ark_bn254::Fr;

use crate::poseidon2::hash;

pub const D_NK: u64 = 1;
pub const D_IVK: u64 = 2;
pub const D_ADDR: u64 = 3;
pub const D_CM: u64 = 4;
pub const D_NF: u64 = 5;

pub fn derive_nk(sk: Fr) -> Fr {
    hash(&[sk, Fr::from(D_NK)])
}

pub fn derive_ivk(sk: Fr) -> Fr {
    hash(&[sk, Fr::from(D_IVK)])
}

pub fn derive_addr(ivk: Fr, d: Fr) -> Fr {
    hash(&[ivk, d, Fr::from(D_ADDR)])
}

pub fn note_commit(value: Fr, asset: Fr, addr: Fr, rho: Fr, rseed: Fr) -> Fr {
    hash(&[value, asset, addr, rho, rseed, Fr::from(D_CM)])
}

pub fn nullifier(nk: Fr, cm: Fr, pos: Fr) -> Fr {
    hash(&[nk, cm, pos, Fr::from(D_NF)])
}
