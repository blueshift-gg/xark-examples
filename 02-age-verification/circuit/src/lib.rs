//! Prove you're 18 or older against an issuer's commitment to your birth date,
//! revealing neither the date nor anything else. Public inputs, in order:
//! `commitment`, `policy`, `holder`. The program builds `policy` from its clock and
//! `holder` from the transaction's signer, so a proof made for another day, or by
//! another wallet, doesn't verify.
use xark::prelude::*;
use xark_hash::Hash;

/// What an issuer attests about a holder, committed to as a whole with Poseidon2.
#[input]
pub struct Identity {
    pub born: Date,
    /// The issuer's random blinding secret: it hides the commitment.
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
    #[cfg(not(xark))]
    pub fn adult_on(today: impl Into<Date>) -> Policy {
        Policy {
            born_by: today.into().years_before(18),
        }
    }
}

/// `holder` is the wallet the proof is for: a Solana address, 32 bytes, as xark's
/// `Hash`. Nothing else uses it, so it's bound with one constraint per half (Tornado's
/// `recipientSquare`): Groth16 checks a public input only through the constraints
/// that use it, and `xark build` refuses one that none do. Without it, anyone who
/// saw the proof could submit it as their own.
#[circuit]
pub fn age_verification(
    id: Private<Identity>,
    commitment: Public<Field>,
    policy: Public<Policy>,
    holder: Public<Hash>,
) {
    require_eq(id.commitment(), commitment);
    require(id.born <= policy.born_by);
    let mut halves = [Field::from(0u64); 2];
    xark::input::write_fields(&holder, &mut halves);
    for half in halves {
        let _ = half * half;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn holder(born: &str) -> Identity {
        Identity {
            born: born.into(),
            salt: Field::from_le_bytes(&[0x2a; 31]),
        }
    }

    const HOLDER: [u8; 32] = [7; 32];

    fn check(id: Identity, today: &str) -> Result<(), xark::prover::ProveError> {
        age_verification(id, id.commitment(), Policy::adult_on(today), HOLDER)
    }

    #[test]
    fn an_adult_passes() {
        check(holder("1990-01-01"), "2026-10-22").unwrap();
        // 18 today.
        check(holder("2008-10-22"), "2026-10-22").unwrap();
    }

    #[test]
    fn a_minor_fails() {
        // 18 tomorrow.
        assert!(check(holder("2008-10-23"), "2026-10-22").is_err());
        // Born on 29 February: 18 on 1 March in a common year.
        assert!(check(holder("2008-02-29"), "2026-02-28").is_err());
        check(holder("2008-02-29"), "2026-03-01").unwrap();
        // Checked on 29 February: 18 years earlier has no 29th, so the 28th.
        check(holder("2010-02-28"), "2028-02-29").unwrap();
        assert!(check(holder("2010-03-01"), "2028-02-29").is_err());
    }

    #[test]
    fn another_identity_fails() {
        let id = holder("1990-01-01");
        let other = holder("1990-01-02").commitment();
        assert!(age_verification(id, other, Policy::adult_on("2026-10-22"), HOLDER).is_err());
    }
}
