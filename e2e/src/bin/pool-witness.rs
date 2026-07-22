//! Witness generator for the shielded pool (example 03) — the wallet side.
//!
//!     cargo run --bin pool-witness -- deposit    # deposit --inputs contents
//!     cargo run --bin pool-witness -- withdraw   # withdraw --inputs contents
//!     cargo run --bin pool-witness -- zeros      # zero-subtree consts + EMPTY_ROOT
//!
//! Models the demo flow: one note (fixtures::SECRET/NULLIFIER) deposited at
//! index 0, withdrawn to the fixture recipient via the fixture relayer, fee 0.
//! The justfile writes the output to a private temporary input file.

use ark_bn254::Fr;
use xark_examples_e2e::{
    fixtures, fr_to_decimal, fr_to_le,
    merkle::{zeros, IncrementalTree, H},
    poseidon2::hash,
    split_pubkey,
};

fn arg(name: &str, v: Fr) -> String {
    format!("{name} = {}", fr_to_decimal(v))
}

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_default();

    let secret = Fr::from(fixtures::SECRET);
    let nullifier = Fr::from(fixtures::NULLIFIER);
    let commitment = hash(&[nullifier, secret]);
    let nullifier_hash = hash(&[nullifier]);

    let mut tree = IncrementalTree::new();
    let old_root = tree.root();
    let frontier = tree.append(commitment);
    let new_root = tree.root();

    let mut out: Vec<String> = Vec::new();
    match mode.as_str() {
        "deposit" => {
            for (i, f) in frontier.iter().enumerate() {
                out.push(arg(&format!("witness.filled_subtrees[{i}]"), *f));
            }
            out.push(arg("statement.old_root", old_root));
            out.push(arg("statement.new_root", new_root));
            out.push(arg("statement.leaf", commitment));
            out.push(arg("statement.index", Fr::from(0u64)));
        }
        "withdraw" => {
            let (path, bits) = tree.auth_path(0);
            let (r_hi, r_lo) = split_pubkey(fixtures::RECIPIENT[32..].try_into().unwrap());
            let (l_hi, l_lo) = split_pubkey(fixtures::RELAYER[32..].try_into().unwrap());
            out.push(arg("witness.secret", secret));
            out.push(arg("witness.nullifier", nullifier));
            for (i, element) in path.iter().enumerate() {
                out.push(arg(&format!("witness.path_elements[{i}]"), *element));
            }
            for (i, bit) in bits.iter().enumerate() {
                out.push(arg(
                    &format!("witness.path_index_bits[{i}]"),
                    Fr::from(*bit as u64),
                ));
            }
            out.push(arg("statement.root", new_root));
            out.push(arg("statement.nullifier_hash", nullifier_hash));
            out.push(arg("statement.recipient_hi", r_hi));
            out.push(arg("statement.recipient_lo", r_lo));
            out.push(arg("statement.relayer_hi", l_hi));
            out.push(arg("statement.relayer_lo", l_lo));
            out.push(arg("statement.fee", Fr::from(0u64)));
        }
        "zeros" => {
            let z = zeros();
            println!("// zeros[i] = root of an all-empty subtree of height i");
            for (i, zi) in z.iter().take(H).enumerate() {
                println!("    Field::from(\"{}\"), // zeros[{i}]", fr_to_decimal(*zi));
            }
            println!("\n// EMPTY_ROOT = zeros[{H}] (root of the all-empty tree), decimal:");
            println!("// {}", fr_to_decimal(z[H]));
            println!("// as 32-byte little-endian (for the program constant):");
            println!("const EMPTY_ROOT: [u8; 32] = {:?};", fr_to_le(z[H]));
            return;
        }
        _ => {
            eprintln!("usage: pool-witness <deposit|withdraw|zeros>");
            std::process::exit(2);
        }
    }
    println!("{}", out.join("\n"));
}
