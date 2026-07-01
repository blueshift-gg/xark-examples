# 02 · Age verification — prove 18+ without revealing your birthday

> Goal: convince a Solana program you're an adult, revealing neither your birth year nor anything
> that identifies you.

This is example [01](../01-over-9000/)'s range proof, now pointed at something real. The trick that
makes it useful is a **commitment**: a public, hiding fingerprint of a secret you can later prove
things about.

## The idea: commit once, prove forever

An **issuer** (a KYC provider, a government portal, or you in a one-time setup) computes a
commitment to your birth year and publishes *only* the commitment:

```
commitment = Poseidon2(birth_year, nonce)
```

- `nonce` is a random blinding value. Without it, an attacker could just hash all ~120 plausible
  birth years and match yours. With it, the commitment reveals nothing.
- The commitment is public and reusable. Any number of times, you prove **"I know the `birth_year`
  and `nonce` behind this commitment, and `current_year − birth_year ≥ 18`."**

## The circuit

```noir
fn main(birth_year: u32, nonce: Field, commitment: pub Field, current_year: pub u32) {
    // 1. Prove we know the opening of the published commitment.
    //    hash2(a, b) = std::hash::poseidon2_permutation([a, b, 0, 0])[0]
    assert(hash2(birth_year as Field, nonce) == commitment);
    // 2. Prove adulthood — range proof, same mechanism as example 01.
    assert(current_year >= birth_year);
    assert(current_year - birth_year >= 18);
}
```

Two public inputs (`commitment`, `current_year`), two private (`birth_year`, `nonce`). The proof
binds the range check to a *specific* commitment — so unlike example 01, you can't just make up a
number. You have to know the opening of a commitment someone published.

## Run it

```bash
# 1. Compute the commitment for your birth_year/nonce (edits are in the circuit)
just commit
#    → paste the printed field element into circuit/Prover.toml as `commitment`

# 2. Prove → export → build
just prove
just export
just build-program

# 3. Verify the proof in a real Solana VM (no deploy needed)
cd ../e2e && cargo test age_verification
#    → age_verification_verifies_on_chain ... ok
```

To deploy to devnet, `solana program deploy` the `.so` and submit the 320-byte
`instruction_data.bin` with any Solana client.

## From demo to production

This example keeps the on-chain side minimal so the ZK part is clear. A real deployment adds two
checks the program stubs out (both noted in `program/src/lib.rs`):

1. **Trust the issuer.** Check the `commitment` public input against a registry of commitments
   signed by issuers you trust — otherwise anyone can commit to `birth_year = 1900` and "prove"
   they're old enough. The ZK proof guarantees *consistency*, not *honesty of the input*.
2. **Trust the clock.** Assert `current_year` matches the on-chain `Clock` sysvar, so a prover
   can't backdate or postdate.

Want to eliminate the trusted issuer entirely? Have the circuit verify an **issuer signature** over
the birth year instead of a commitment — xark supports the `EcdsaSecp256k1`/`EcdsaSecp256r1`
black-boxes (~3.6M/5.4M constraints). That's the verifiable-credentials / zk-KYC upgrade path.

## Poseidon compatibility

On nargo 1.0.0-beta.22 the stdlib exposes only the Poseidon2 *permutation*
(`std::hash::poseidon2_permutation`, the `Poseidon2Permutation` black-box xark supports), so the
circuit builds a fixed-arity compression `hash2(a, b) = poseidon2_permutation([a, b, 0, 0])[0]`.
Because the commitment is computed by Noir's own hash (via `just commit`) and checked by the same
hash in-circuit, the two always agree — there's no cross-system parameter matching to get wrong.

Next: [**03 · Shielded pool →**](../03-shielded-pool/) — Merkle trees, nullifiers, and unlinkable
transfers.
