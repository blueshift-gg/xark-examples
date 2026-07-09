//! Merkle-tree pieces shared by the deposit and withdraw circuits: the tree
//! shape, the empty-subtree roots, the incremental-append recomputation, and a
//! branchless mux. All hashing is `xark_poseidon2::hash2` (capacity tag 0), so
//! tree nodes live in a different Poseidon2 domain than note commitments
//! (`hash::<2>`, tag 2) and nullifier hashes (`hash::<1>`, tag 1).
#![no_std]

use xark::Field;
use xark_poseidon2::hash2;

/// Tree height → 2^H leaves.
pub const H: usize = 20;

/// Nothing-up-my-sleeve empty-leaf sentinel: ASCII `xarkzero` as a u64.
/// A nonzero domain constant prevents a valid zero field value from being
/// confused with an unoccupied leaf.
pub const ZERO_LEAF: u64 = 0x7861_726b_7a65_726f;

/// `zeros()[i]` = root of an all-empty subtree of height `i`. Precomputed
/// constants, like Tornado's hard-coded zeros — computing the chain in-circuit
/// would cost ~7.6k constraints. Regenerate with
/// `cargo run --bin pool-witness -- zeros` (e2e crate).
pub fn zeros() -> [Field; H] {
    [
        Field::from("8674340163232821871"), // zeros[0] = ZERO_LEAF
        Field::from("9042127482412114750280779369703900095668486295914540157541994865457556438788"),
        Field::from("341054539363681503553976831131094549576226458796491400306836911988583808090"),
        Field::from("34348404945049038236388505945633326005442107038298488892105321413384981752"),
        Field::from("2607586432627782529895911255360599119305717852316899644428014773550259711648"),
        Field::from(
            "17143657303453559283748401277293741006494718939520447373178310356951340656726",
        ),
        Field::from(
            "14585961217425837916146007748409581978138376449198799174701643770748187944420",
        ),
        Field::from(
            "12913104259683958615504569307985232852859093081747457753745898074093837086031",
        ),
        Field::from("6654419737733171058816783470728446315513530076756729386694119633228614541677"),
        Field::from("9905653235998198959267229318200180305527472016019263164456678526227841750527"),
        Field::from("1795524235043139340704045661862400299784465129234003853482408229783646380153"),
        Field::from(
            "19983021878367582322503868342355183464266102410038630684551169236950558235765",
        ),
        Field::from(
            "15493636575207974485733064366466373244908560743446661717288277285308080854036",
        ),
        Field::from(
            "14083167620132500000591427448186996844325363966164438075903688512980458224553",
        ),
        Field::from("4091336417334400288428247233993014712986282462794093160765325429720863643452"),
        Field::from(
            "19431590493375067142136389840073142521048107757278094378966114213855590690690",
        ),
        Field::from(
            "20811605721516896163935571191091374107901846715636461739988745852635934187625",
        ),
        Field::from("106471845104681561116946404869270087809661202083039934305723705992379928635"),
        Field::from(
            "21019561680803183314549017725802214759305623308395087484566269367878397611521",
        ),
        Field::from(
            "14684084919228591460864761508397974036133516905271292685778887624640406561500",
        ),
    ]
}

/// Branchless `if cond { a } else { b }` for a boolean `cond` wire.
pub fn mux(cond: Field, a: Field, b: Field) -> Field {
    b + cond * (a - b)
}

/// Root of the tree when position `index` (given LSB-first as `index_bits`)
/// holds `leaf`, everything after it is empty, and `filled[i]` are the
/// left-siblings along the path — the standard incremental-Merkle
/// recomputation (à la Tornado's MerkleTreeWithHistory), lifted into a circuit.
///
/// `index_bits` must be constrained boolean by the caller (e.g. produced by
/// `to_bits`, which enforces booleanity).
pub fn root_with_leaf(
    leaf: Field,
    index_bits: [Field; H],
    filled: [Field; H],
    zeros: [Field; H],
) -> Field {
    let mut current = leaf;
    let mut i = 0usize;
    while i < H {
        let b = index_bits[i];
        // bit 0: current is a LEFT child, its right sibling is an empty subtree;
        // bit 1: current is a RIGHT child, its left sibling is already filled.
        let l = mux(b, filled[i], current);
        let r = mux(b, current, zeros[i]);
        current = hash2(l, r);
        i += 1;
    }
    current
}

/// Fold a leaf up its authentication path (membership proof). `path[i]` is the
/// sibling at height `i`; `index_bits[i]` says whether the current node is the
/// right child. Booleanity of `index_bits` is asserted here.
pub fn merkle_root(leaf: Field, path: [Field; H], index_bits: [Field; H]) -> Field {
    let mut current = leaf;
    let mut i = 0usize;
    while i < H {
        let b = index_bits[i];
        xark::lang::assert_eq(b * b, b); // b ∈ {0, 1}
        let l = mux(b, path[i], current);
        let r = mux(b, current, path[i]);
        current = hash2(l, r);
        i += 1;
    }
    current
}
