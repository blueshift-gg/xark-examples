// Over 9000 — the "hello world" of ZK range proofs.
//
// We prove: "I know a secret `power_level` that is greater than 9000."
// The verifier learns *only* that fact — never the number itself.
//
// This circuit has ZERO public inputs: the threshold (9000) is baked into the
// circuit, so it's fixed by the verifying key. That makes the on-chain proof
// the smallest possible thing to submit (just the 256-byte proof, no public
// inputs). Examples 02 and 03 add public inputs to make range proofs *useful*
// (age gates, private balances). This one is deliberately, gloriously silly.
//
// Under the hood: `power_level: u64` forces `power_level` into the range
// [0, 2^64). The comparison `> 9000` lowers to ACIR's `RANGE` black-box, which
// xark supports natively. That's the entire mechanism behind every range proof
// you'll ever write.
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
