# 02 · Age verification — prove 18+ without a birthday

Prove to a Solana program that you're an adult, revealing neither your birth year nor anything that
identifies you. This is example [01](../01-over-9000/)'s range proof pointed at something real — and
the tool that makes it real is a **commitment**.

## Commit once, prove many times

An issuer (a KYC provider, a government portal, or you in a one-time setup) hashes your birth year
with a random nonce and publishes only the result:

```
commitment = Poseidon2(birth_year, nonce)
```

- The **nonce** is what makes the commitment *hiding*. There are only ~120 plausible birth years;
  without a nonce an attacker just hashes all of them and matches yours. A ≥128-bit nonce makes that
  search hopeless, so the commitment leaks nothing.
- The commitment is **public and reusable**. Any number of times, you prove *"I know the `birth_year`
  and `nonce` behind this commitment, and `current_year − birth_year ≥ 18`"* — without reopening it.

## The circuit

```noir
fn main(birth_year: u32, nonce: Field, commitment: pub Field, current_year: pub u32) {
    // Prove we know the opening of the published commitment.
    // hash2(a, b) = std::hash::poseidon2_permutation([a, b, 0, 0])[0]
    assert(hash2(birth_year as Field, nonce) == commitment);
    // Then the range check — same mechanism as example 01.
    assert(current_year >= birth_year);
    assert(current_year - birth_year >= 18);
}
```

Two public inputs (`commitment`, `current_year`), two private (`birth_year`, `nonce`). The first
assertion is the load-bearing one: it binds the range check to a *specific published commitment*, so
unlike example 01 you can't invent a convenient number — you have to know the opening of a commitment
someone already trusts.

The idea to carry forward: **a ZK proof guarantees consistency, not honesty.** It proves the birth
year inside the commitment is at least 18 years ago; it says nothing about whether the commitment
itself is truthful. That's a trust decision the program makes — next.

## From demo to production

The on-chain side is kept minimal so the ZK part is legible. A real deployment adds two checks the
program stubs out (both flagged in `program/src/lib.rs`):

1. **Trust the issuer.** Check `commitment` against a registry of commitments signed by issuers you
   trust — otherwise anyone commits to `birth_year = 1900` and "proves" they're old enough.
2. **Trust the clock.** Assert `current_year` matches the on-chain `Clock` sysvar, so a prover can't
   back- or post-date.

Want to drop the trusted issuer entirely? Have the circuit verify an **issuer signature** over the
birth year instead of a commitment — xark supports the `EcdsaSecp256k1`/`EcdsaSecp256r1` black-boxes
(~3.6M / 5.4M constraints). That's the verifiable-credentials / zk-KYC path.

## Run it

```bash
just commit         # compute Poseidon2(birth_year, nonce) with Noir's own hash
                    #   → paste the printed field into circuit/Prover.toml as `commitment`
just prove
just export
just build-program
cd ../e2e && cargo test age_verification   # verify in a real Solana VM
```

`just commit` computes the commitment with the *same* Poseidon the circuit uses, so the two always
agree — there's no cross-tool hash-parameter matching to get wrong. (On nargo beta.22 the stdlib
exposes only the Poseidon2 permutation, so `hash2` is a fixed-arity compression over it:
`poseidon2_permutation([a, b, 0, 0])[0]`.)

To deploy to devnet, `solana program deploy` the `.so` and submit the 320-byte `instruction_data.bin`
(256-byte proof + two 32-byte public inputs).

Next → [03 · Shielded pool](../03-shielded-pool/)
