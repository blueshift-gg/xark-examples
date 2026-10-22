# 02 · Age verification: prove 18+ without a birth date

Prove to a Solana program that you're 18 or older today, revealing neither your birth date nor
anything that identifies you. This is [01](../01-over-9000/)'s range proof pointed at something
real, and two things make it real: a **commitment**, so the date can't be made up, and the
**program's clock**, so "today" can't either.

## Commit once, prove many times

An issuer (a KYC provider, a government portal) commits to your identity and publishes only the
result:

```
commitment = Poseidon2(born, salt)
```

- The **salt** makes the commitment *hiding*. There are only ~40,000 plausible birth dates; without
  a salt an attacker hashes all of them and matches yours. A random 248-bit salt makes that search
  hopeless, so the commitment leaks nothing.
- The commitment is **public and reusable**. Any number of times, you prove *"I know the identity
  behind this commitment, and I was born on or before this day 18 years ago"*, without reopening it.

## The circuit

```rust
use xark::prelude::*;
use xark_hash::Hash;

#[input]
pub struct Identity {
    pub born: Date,
    pub salt: Field,
}

#[input]
impl Identity {
    /// The issuer's commitment: what a verifier sees of the holder, and all it sees.
    pub fn commitment(&self) -> Field {
        xark_poseidon2::hash(self.to_fields())
    }
}

/// What a verifier requires: born on or before `born_by`.
#[input]
pub struct Policy {
    pub born_by: Date,
}

#[input]
impl Policy {
    /// 18 or older on `today`: born on or before the same day 18 years earlier.
    #[cfg(not(xark))] // host code (and the client): the circuit only compares
    pub fn adult_on(today: impl Into<Date>) -> Policy {
        Policy { born_by: today.into().years_before(18) }
    }
}

#[circuit]
pub fn age_verification(
    id: Private<Identity>,
    commitment: Public<Field>,
    policy: Public<Policy>,
    holder: Public<Hash>,
) {
    require_eq(id.commitment(), commitment);
    require(id.born <= policy.born_by);
    // bind `holder`: one constraint per half (below)
}
```

It's [xark's `zkyc`](https://github.com/blueshift-gg/xark/blob/remove-nightly-compiler/examples/zkyc/src/lib.rs)
cut down to one rule. Everything is a typed input:

- **`Identity` and `Policy` are `#[input]` structs**, in the circuit, in tests, and in the TypeScript
  client, which gets `Identity { born: Date, salt }` and `Policy { bornBy: Date }`.
- **`Date` compares as a date**: `id.born <= policy.born_by` is a 27-bit comparison, with no
  arithmetic on the encoding.
- **`#[input] impl` exports the Rust to the client.** The issuer computes the commitment with the
  very function the circuit checks it with (`Identity.commitment(id)`), and the holder builds the
  program's policy with `Policy.adultOn(today)`. There's no second implementation to drift.

The first requirement is the load-bearing one: it binds the range check to a *specific published
commitment*, so unlike 01 you can't pick a convenient birth date. You have to know the opening of a
commitment someone already trusts.

The idea to carry forward: **a ZK proof guarantees consistency, not honesty.** It proves the date
inside the commitment is old enough; it says nothing about whether the commitment is truthful. That
is a trust decision the program makes (see below).

## The proof is for one wallet

Without `holder`, a proof says *someone* holding this credential is an adult, and anyone who sees it
on-chain could submit it as their own. So the holder's wallet is a public input: a Solana address
is 32 bytes, which the circuit takes as xark's `Hash` (two 16-byte halves). The program fills it
from the transaction's signer, so the proof only verifies when the wallet it was made for signs.

Nothing in the circuit computes with `holder`, and that's a trap: Groth16 checks a public input only
through the constraints that use it, so an unused one would verify with any value. `xark build`
refuses such a circuit, and the fix is one constraint per half, Tornado's `recipientSquare`
(`half * half`).

## The program supplies the rest

The [program](./program/src/lib.rs) takes the holder as its signer and `proof ‖ commitment` as
instruction data, and supplies the other public inputs itself: `Policy::adult_on(today)`, with today's UTC date from the `Clock` sysvar
(Howard Hinnant's `civil_from_days`). The program can't link xark, so it computes `born_by` in a few
lines of `no_std` Rust; its tests check both against the calendar and against the circuit's
`Policy::adult_on` for every day across 300 years. A prover can't backdate or postdate, and a proof
made today stops verifying tomorrow. The [test](./client/age-verification.test.ts) shows each case:
the proof verifies today for its wallet, and fails for another wallet, for someone else's
commitment, and the next day.

```ts
const commitment = Identity.commitment(id);   // the issuer's
const policy = Policy.adultOn(new Date());    // the program's rule, from the circuit's Rust
const proof = prover.prove({ id, commitment, policy, holder: walletBytes });
// a transaction signed by the holder, data: proof.toBytes() ‖ commitment.toBytes()
```

A proof is made for a UTC day: one that lands after midnight UTC needs proving again.

## From demo to production

**Trust the issuer.** The program here accepts any commitment. A real one checks it against a
registry of commitments issuers you trust have signed; otherwise anyone commits to a 1900 birth date
and "proves" they're old enough. To drop the registry entirely, have the circuit verify an
**issuer's signature** over the identity instead: xark ships secp256k1/secp256r1 ECDSA and Ed25519
gadget crates, and its `zkyc` example is this circuit grown into a full KYC policy (nationality, net
worth, document expiry).

## Run it

```bash
just build          # circuit → dev keys → verifier crate → client → .so
just test-circuit   # adult, 18 today, 18 tomorrow, 29 February, another identity
cd .. && npx vitest run 02   # issue, prove with the client, verify on-chain (LiteSVM)
```

Next → [03 · Shielded pool](../03-shielded-pool/)
