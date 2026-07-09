//! The "hello world" of ZK range proofs: prove a secret value is > 9000 without
//! revealing it. The `u64`-typed comparison bound is what makes `>` sound — it
//! range-checks `power_level` to 64 bits before ordering. Zero public inputs
//! (the `9000` literal is baked into the verifying key), so the on-chain proof
//! is just 256 bytes. (Full walkthrough in the README.)
#![cfg_attr(not(test), no_std)]

use xark::prelude::*;

#[circuit]
pub fn circuit(power_level: Private<Field>) {
    assert(power_level > 9000u64);
}

#[cfg(test)]
mod tests {
    use super::CircuitInputs;

    // Loads target/xark/over_9000/ — run via `xark test` (or `xark build` first).
    fn over_9000() -> xark_prover::Circuit {
        xark_prover::circuit("over_9000")
    }

    #[test]
    fn it_is_over_9000() {
        assert!(over_9000().prove(CircuitInputs { power_level: 9001 }));
    }

    #[test]
    fn it_is_not_over_9000() {
        assert!(!over_9000().prove(CircuitInputs { power_level: 9000 }));
    }
}
