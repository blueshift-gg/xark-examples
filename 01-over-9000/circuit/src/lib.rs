//! The "hello world" of ZK range proofs: prove a secret value is greater than
//! 9000 without revealing it. The `u64` suffix supplies the comparison width,
//! and Xark enforces that range in-circuit. There are no public inputs: `9000`
//! is baked into the verifying key, so the on-chain proof is exactly 256 bytes.
use xark::prelude::*;

#[circuit]
pub fn over_9000(power_level: Private<u64>) {
    require(power_level > 9000u64);
}

#[cfg(test)]
mod tests {
    use super::over_9000;

    #[test]
    fn accepts_a_value_over_9000() {
        over_9000(9001).unwrap();
    }

    #[test]
    fn rejects_the_boundary() {
        assert!(over_9000(9000).is_err());
    }
}
