// The "hello world" of ZK range proofs: prove a secret value is > 9000 without
// revealing it. `u64` + `>` lowers to ACIR's RANGE black-box; zero public inputs
// means the on-chain proof is just 256 bytes. (Full walkthrough in the README.)
fn main(power_level: u64) {
    assert(power_level > 9000);
}

#[test]
fn it_is_over_9000() {
    main(9001);
}

#[test(should_fail)]
fn it_is_not_over_9000() {
    main(9000);
}
