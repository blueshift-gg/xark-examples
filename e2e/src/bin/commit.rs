//! Compute the age-verification commitment `Poseidon2(birth_year, nonce)` —
//! the value you publish on-chain and pass to `xark prove` as `commitment`.
//!
//!     cargo run --bin commit -- <birth_year> <nonce-decimal-or-0xhex>
//!
//! Uses the same construction as the circuit (`hash2` from the xark gadget),
//! so the two can never disagree.

use ark_bn254::Fr;
use ark_ff::PrimeField;
use xark_examples_e2e::{fr_to_decimal, poseidon2::hash2};

fn parse_fr(s: &str) -> Fr {
    if let Some(hex) = s.strip_prefix("0x") {
        let hex = if hex.len() % 2 == 1 {
            format!("0{hex}")
        } else {
            hex.to_string()
        };
        let mut bytes: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("hex field element"))
            .collect();
        bytes.reverse(); // big-endian hex → little-endian bytes
        Fr::from_le_bytes_mod_order(&bytes)
    } else {
        use std::str::FromStr;
        Fr::from_str(s).expect("decimal field element")
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [birth_year, nonce] = args.as_slice() else {
        eprintln!("usage: commit <birth_year> <nonce (decimal or 0xhex)>");
        std::process::exit(2);
    };
    let commitment = hash2(parse_fr(birth_year), parse_fr(nonce));
    println!("{}", fr_to_decimal(commitment));
}
