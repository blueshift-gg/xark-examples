//! Client-side helpers shared by the e2e tests and the small CLI bins: the
//! native Poseidon2 that matches the circuits, plus field encode/decode
//! utilities. This is what a real wallet for these examples would vendor.

pub mod merkle;
pub mod notes;
pub mod poseidon2;

use ark_bn254::Fr;
use ark_ff::{BigInteger, PrimeField};

/// A field element as the 32-byte little-endian wire format every xark
/// artifact (proofs, public inputs, instruction data) uses.
pub fn fr_to_le(x: Fr) -> [u8; 32] {
    let mut out = [0u8; 32];
    out.copy_from_slice(&x.into_bigint().to_bytes_le());
    out
}

/// Parse the 32-byte little-endian wire format back into a field element.
pub fn fr_from_le(b: &[u8; 32]) -> Fr {
    Fr::from_le_bytes_mod_order(b)
}

/// Decimal string form used by `xark prove --inputs`.
pub fn fr_to_decimal(x: Fr) -> String {
    x.into_bigint().to_string()
}

/// Split a 32-byte pubkey into two 128-bit field elements, exactly as the
/// shielded-pool program does: `lo` = bytes[0..16], `hi` = bytes[16..32].
pub fn split_pubkey(pk: &[u8; 32]) -> (Fr, Fr) {
    let hi = Fr::from_le_bytes_mod_order(&pk[16..32]);
    let lo = Fr::from_le_bytes_mod_order(&pk[..16]);
    (hi, lo)
}

/// Shared demo fixtures: the note secrets and the keypairs bound into the
/// withdraw proof. Used by the witness bins (proving side) and the e2e tests
/// (on-chain side) so the two always agree.
pub mod fixtures {
    /// Demo note (never do this with real funds — secrets belong in a wallet).
    pub const SECRET: u64 = 1111;
    pub const NULLIFIER: u64 = 2222;

    /// 64-byte ed25519 keypairs (secret ‖ pubkey). The pubkey halves are bound
    /// into the withdraw proof; the secret halves let the e2e sign as them.
    pub const RELAYER: [u8; 64] = [
        222, 27, 248, 241, 243, 155, 183, 138, 72, 86, 34, 113, 80, 238, 215, 7, 219, 136, 30, 144,
        122, 237, 63, 116, 254, 188, 128, 113, 105, 153, 172, 56, 91, 240, 156, 105, 6, 194, 63,
        197, 56, 125, 221, 53, 2, 248, 135, 176, 89, 78, 14, 41, 204, 168, 171, 198, 70, 69, 81,
        57, 44, 158, 77, 232,
    ];
    pub const RECIPIENT: [u8; 64] = [
        212, 15, 49, 134, 72, 242, 239, 139, 44, 143, 167, 93, 16, 39, 229, 29, 13, 215, 136, 22,
        134, 121, 177, 47, 4, 83, 95, 86, 234, 242, 171, 109, 137, 187, 244, 226, 218, 76, 53, 133,
        74, 6, 206, 253, 228, 242, 220, 84, 148, 212, 171, 202, 211, 57, 164, 158, 9, 139, 238,
        207, 95, 214, 211, 43,
    ];

    /// Example 04's fixed SPL mint. The circuit's `asset` tag is its low 16
    /// bytes as a field element.
    pub const MINT_PUBKEY: [u8; 32] = [
        123, 236, 127, 84, 75, 150, 142, 114, 96, 47, 36, 185, 180, 0, 184, 185, 219, 76, 85, 214,
        104, 64, 213, 210, 101, 226, 248, 208, 110, 243, 181, 54,
    ];

    /// Fixed token-account address bound into example 04's proof chain.
    pub const BOB_TOKEN_PUBKEY: [u8; 32] = [
        171, 81, 95, 232, 242, 89, 248, 164, 217, 166, 117, 91, 240, 241, 11, 160, 74, 9, 72, 60,
        220, 150, 10, 204, 159, 222, 63, 182, 183, 102, 167, 144,
    ];
}
