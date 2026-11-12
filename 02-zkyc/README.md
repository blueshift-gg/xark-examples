# 02 · zKYC: prove you pass KYC without showing your ID

Prove to a Solana program that you meet its KYC policy (18 or older today, worth at least 500,000,
a national of a country it allows, holding a document that hasn't expired), revealing nothing
else: not your birth date, your net worth, your nationality or your document number. This is
[01](../01-over-9000/)'s range proof put to work: the age is a range proof on dates, the minimum
net worth one on amounts. Three more tools keep the rest private: an **allow-list check**, so the
program learns your country is on its list but not which one; a **salted commitment**, so the
identity can't be made up and its only trace onchain reveals nothing; and the **signer**, so the
proof can't be borrowed. The **program's policy** makes sure the rules can't be made up either.

## Commit once, prove many times

An issuer (a KYC provider, a government portal) checks your documents once, commits to what it
found, and publishes only the result:

```
commitment = commit(document, born ‖ nationality ‖ net_worth ‖ expires, salt)
```

`commit` is `xark_poseidon2::commit`: the Poseidon2 sponge in a domain of its own, so no other
Poseidon2 hash of the same fields (a nullifier, a Merkle node) equals it.

- The **salt** makes the commitment *hiding*. Birth dates, countries and round net worths are few
  enough to guess; without a salt an attacker hashes every plausible identity and matches yours. A
  random 248-bit salt makes that search hopeless, so the commitment leaks nothing.
- The commitment is **public and reusable**. Any number of times, for any verifier, you prove *"I
  know the identity behind this commitment, and it meets this policy"*, without reopening it.

## The circuit

```rust
use xark::prelude::*;

/// A day this circuit's dates can be: 1900-01-01 to 2100-12-31, 17 bits.
pub type Day = Days<1900, 2100>;

#[input]
pub struct Identity {
    pub document: Text,
    pub born: Day,
    pub nationality: Choice<Country>,
    pub net_worth: u64,
    pub expires: Day,
    pub salt: Field,
}

#[input]
impl Identity {
    /// The issuer's commitment: Poseidon2's `commit` over every field, so adding one
    /// extends it.
    pub fn commitment(&self) -> Field {
        xark_poseidon2::commit(self.to_fields())
    }
}

/// What a verifier requires, checked on `today`.
#[input]
pub struct Policy {
    pub today: Day,
    pub born_by: Day,
    pub min_net_worth: u64,
    pub allowed: Bitfield<Country>,
}

#[circuit]
pub fn zkyc(
    id: Private<Identity>,
    commitment: Public<Field>,
    policy: Public<Policy>,
    holder: Public<Address>,
) {
    require_eq(id.commitment(), commitment);
    require(id.born <= policy.born_by);
    require(id.net_worth >= policy.min_net_worth);
    require(id.expires >= policy.today);
    id.nationality.require_in(policy.allowed);
    bind(holder);
}
```

It's [xark's `zkyc`](https://github.com/blueshift-gg/xark/blob/master/examples/zkyc/src/lib.rs)
with a holder added (below) and the commitment made with `commit`. The
[full circuit](./circuits/zkyc/src/lib.rs) also has `Policy`'s builder
(`Policy::default().on(today).min_age(18).worth_at_least(500_000).nationalities([US, GB, FR, DE])`)
and the tests.

The first requirement is the load-bearing one: it binds every other check to a *specific published
commitment*, so unlike 01 you can't pick convenient values. You have to know the opening of a
commitment someone already trusts. Each other line proves a rule and hides the value it checks:

- **Age is a range proof on dates.** A `Day` is a day count since 1900-01-01, range-checked to 17
  bits on entry, so `id.born <= policy.born_by` is 01's comparison on two 17-bit numbers. The
  verifier learns "born on or before `born_by`", never the birth date. The expiry check works the
  same way.
- **Minimum net worth is a 64-bit range proof.** `id.net_worth >= policy.min_net_worth` proves at
  least the minimum, never the amount.
- **The allow-list hides which country.** `require_in` splits the public `Bitfield` into one bit per
  country and multiplies one factor per country, each zero only where the private `Choice` is that
  country and its bit is set; the product must be zero. The verifier sees the allowed set, not
  which member of it you are.

What goes onchain is the proof and the commitment, in the instruction data, and the holder's
address as the signer. The program stores nothing; the identity and its salt stay with the holder
and the issuer.

The idea to carry forward: **a ZK proof guarantees consistency, not honesty.** It proves the
identity inside the commitment meets the policy; it says nothing about whether the commitment is
truthful. That is a trust decision the program makes (see below).

## Typed inputs

Nothing here is a bare `Field` the author encodes by hand. Each `#[input]` type knows its own
range, and the TypeScript client takes it in its natural form:

| Type | In the circuit | In the client |
|---|---|---|
| `Text` | up to 31 bytes of UTF-8 in one field element | `"A1938274"` |
| `Days<1900, 2100>` | days since 1900-01-01, 17 bits, compared as dates | a JS `Date`, read in UTC |
| `Choice<Country>` | one ISO 3166 code: its position among 249, 8 bits | `Country.US`, or `"US"` |
| `Bitfield<Country>` | a set of codes: bit `i` for the country at position `i` | `["DE", "FR", "GB", "US"]` |
| `u64` | range-checked to 64 bits on entry | a number or a `bigint` |
| `Address` | a Solana address: two field elements, its 16-byte halves | @solana/kit's `Address` |

`id.nationality.require_in(policy.allowed)` is a set membership check over all 249 countries, one
factor each; with the commitment's Poseidon2 it's most of the circuit's 1,350 constraints.

`#[input] impl` exports the Rust to the client. The issuer computes the commitment with the very
function the circuit checks it with (`Identity.commitment(id)`), and anyone builds a policy with
the same chain as in Rust: `Policy.default().on(today).minAge(18).worthAtLeast(500_000)`. There's
no second implementation to drift.

## Packing

A Groth16 public input is a field element, and each one costs the program a scalar
multiplication. So `#[input]` packs a struct's consecutive fixed-width fields into one element, the
first in the lowest bits, while they fit in 253 bits (every value below 2²⁵³ is a distinct BN254
scalar, so a packed element splits back one way only). In the circuit a packed element is split
into its bits once, which is also each field's range check.

`Policy` is two public inputs, not four:

```
policy[0] = today | born_by << 17 | min_net_worth << 34    (17 + 17 + 64 = 98 bits)
policy[1] = allowed                                         (a Bitfield: an element of its own)
```

`Identity` packs the same way: `born`, `nationality`, `net_worth` and `expires` share one element,
so its commitment hashes three. `xark inspect --json` lists every public input leaf by
leaf, in proof order, and names the packed ones by their fields:

```
commitment
policy.{today,born_by,min_net_worth}
policy.allowed
holder.hi
holder.lo
```

The program has to produce those exact bytes, but it doesn't pack them by hand: `xark build`
generates `PublicInputs` in the verifier crate, which packs from the circuit's schema as `#[input]`
does. The program's tests check it against the circuit's `Policy::to_fields()` for every day from
1918 to 2100.

## A policy is a public input

The rules aren't constants in the circuit: they're `policy`, a public input. So every verifier
picks its own, a different minimum age, no net-worth check, a longer list of countries, with the
same circuit and the same key. `Policy::default()` passes everyone, and each builder method
tightens one check.

That only means something if the **verifier** supplies the policy. A proof says *"this identity
meets **this** policy"*; a holder who's too poor can make a perfectly valid proof against a policy
with no net-worth check. So the program never reads a policy from the transaction: it builds its
own, and the proof verifies only if the holder proved against exactly that.

## The proof is for one wallet

Without `holder`, a proof says *someone* holding this credential passes, and anyone who sees it
onchain could submit it as their own. So the holder's wallet is a public input, an `Address`:
xark's Solana address, two field elements, its 16-byte halves. The client takes the wallet's
address as @solana/kit has it, and the program fills it from the transaction's signer, so the proof
only verifies when the wallet it was made for signs.

Nothing in the circuit computes with `holder`, and that's a trap: Groth16 checks a public input only
through the constraints that use it, so an unused one would verify with any value. `xark build`
refuses such a circuit. `bind(holder)` is the fix: one constraint per half, Tornado's
`recipientSquare` (`half * half`).

## The program supplies the rest

The [program](./programs/zkyc/src/) has one instruction, `VerifyKyc`: the holder signs, and the
instruction data is its discriminator `0`, then `proof ‖ commitment`. It assembles every other public
input itself.

```
programs/zkyc/src/
  lib.rs                      entrypoint: dispatch on the discriminator, log the error's name
  instructions/verify_kyc.rs  the accounts (the holder, a signer), the data, verify
  policy.rs                   the program's policy as the circuit's `Policy`, and the calendar
  errors.rs                   ZkycError::InvalidProof (0)
  tests.rs                    host tests
```

A missing account, an unsigned holder or malformed data fail with Solana's built-in errors
(`NotEnoughAccountKeys`, `MissingRequiredSignature`, `InvalidInstructionData`); a proof that
doesn't verify is the program's own `InvalidProof`, custom error 0, whose name it logs. Its policy
is three constants and the clock:

```rust
use zkyc_verifier::{Bitfield, Country, Days, Policy, PublicInputs, verify};

const MIN_AGE: i64 = 18;
const MIN_NET_WORTH: u64 = 500_000;
const ALLOWED: Bitfield<Country> = Bitfield::EMPTY.with(DE).with(FR).with(GB).with(US);

let today = Days::<1900, 2100>::from_unix(clock.unix_timestamp)?; // today, UTC
let inputs = PublicInputs {
    commitment: *commitment,
    // born_by: the same day MIN_AGE years earlier (the 28th for a missing 29 February)
    policy: Policy { today, born_by: born_by(today)?, min_net_worth: MIN_NET_WORTH, allowed: ALLOWED },
    holder: *holder.address().as_array(), // an `Address`: split into hi and lo for you
};
if !verify(&proof, inputs) { … }
```

The verifier crate `xark build` generates ([`clients/zkyc/rust`](./clients/zkyc/rust/src/lib.rs))
gives the program the circuit's types without linking xark: `Days` for the dates, `Bitfield<Country>`
for the allow-list, and `PublicInputs`, which packs them. A minimum age is the one thing it leaves to
the program, the calendar math `min_age` does in the circuit's Rust, so `born_by` is a few lines of
`no_std` Rust (Howard Hinnant's `civil_from_days`). Its tests check the calendar against fixed dates,
the policy against the circuit's own for every day from 1918 to 2100, `ALLOWED` against
`Bitfield::of([DE, FR, GB, US])`, and the demo proof
([`circuits/zkyc/examples/demo.rs`](./circuits/zkyc/examples/demo.rs), the verifier crate's sample)
against what it assembles: it verifies for its holder and day, and not for another wallet, another
day or another commitment.

A prover can't loosen the rules, backdate or postdate, and a proof made today stops verifying
tomorrow. The [test](./tests/zkyc.test.ts) shows each case: the proof verifies today for its
wallet, and fails for another wallet, for someone else's commitment, the next day, and when made
against a laxer policy than the program's. An identity that fails the policy (too young, too poor,
the wrong nationality, an expired document) has no proof at all: the client throws `CheckFailed`
naming the line.

```ts
const commitment = Identity.commitment(id);   // the issuer's
const policy = Policy.default().on(new Date()).minAge(18).worthAtLeast(500_000).nationalities(["DE", "FR", "GB", "US"]);
const proof = prover.prove({ id, commitment, policy, holder: wallet.address });
// a transaction signed by the holder, data: [0] ‖ proof.toBytes() ‖ commitment.toBytes()
```

A proof is made for a UTC day: one that lands after midnight UTC needs proving again.

## From demo to production

**Trust the issuer.** The program here accepts any commitment. A real one checks it against a
registry of commitments issuers you trust have signed; otherwise anyone commits to a 1900 birth date
and a billion in net worth. To drop the registry entirely, have the circuit verify an **issuer's
signature** over the identity instead: xark ships secp256k1/secp256r1 ECDSA and Ed25519 gadget
crates.

**Make the policy configurable.** Constants keep the example short. A program serving several
verifiers would keep each policy (minimum age, net worth, allowed countries) in an account its
owner sets, and build its `Policy` the same way.

## Numbers

| Constraints | Public inputs | Prove, 1 thread | Prove, 8 workers | Onchain CU |
|--:|--:|--:|--:|--:|
| 1,350 | 5 | 211 ms | 70 ms | 102,549 |

Measured as in the [root README](../README.md#numbers): `xark inspect`, the generated client in
node, the whole instruction in LiteSVM.

## Run it

```bash
xark build     # circuit, its clients (Rust verifier crate, TypeScript), then the program
xark test      # who passes, who doesn't; the program's policy; then issue, prove, verify onchain
```

`npm run check` is CI's `xark build --check`: the committed keys and clients are what the source
builds. Prerequisites: [../RUNBOOK.md](../RUNBOOK.md) (`xark doctor` checks them).

Next → [03 · Token claim](../03-token-claim/): Merkle trees in a circuit
