#!/usr/bin/env python3
"""Generate e2e/src/poseidon2.rs from xark's Poseidon2 gadget source.

The circuits hash with `xark-poseidon2` (BN254, t=3, alpha=5, R_F=8, R_P=56).
Client-side code (witness generation, commitments, Merkle trees) needs the
*same* hash natively, and the xark DSL has no native runtime — so we transcribe
the gadget's round constants into an arkworks implementation. The layout and
round order mirror the gadget function-for-function, and the generated module
carries a known-answer test pinned to the gadget's own reference vector, so a
constant transcription error cannot slip through.

Usage: scripts/gen_poseidon2_native.py [path-to-xark-checkout]
       (default: resolve the pinned xark-poseidon2 source with cargo metadata)
"""

import json
import re
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
OUT = REPO_ROOT / "e2e" / "src" / "poseidon2.rs"


def find_gadget_src() -> Path:
    if len(sys.argv) > 1:
        path = Path(sys.argv[1]) / "crates" / "xark-poseidon2" / "src" / "lib.rs"
        if path.is_file():
            return path
        sys.exit(f"xark-poseidon2 source not found at {path}")

    manifest = REPO_ROOT / "02-age-verification" / "circuit" / "Cargo.toml"
    result = subprocess.run(
        [
            "cargo",
            "metadata",
            "--format-version",
            "1",
            "--manifest-path",
            str(manifest),
            "--locked",
        ],
        check=True,
        capture_output=True,
        text=True,
    )
    metadata = json.loads(result.stdout)
    package = next(p for p in metadata["packages"] if p["name"] == "xark-poseidon2")
    return Path(package["manifest_path"]).parent / "src" / "lib.rs"


def main() -> None:
    src_path = find_gadget_src()
    src = src_path.read_text()

    # All decimal literals inside poseidon2_perm's constant tables, in source
    # order: 24 external (8 rounds x 3 lanes), then 56 internal.
    body = src[src.index("let rc_ext") : src.index("// Initial external linear layer")]
    decs = re.findall(r'"(\d+)"', body)
    assert len(decs) == 24 + 56, f"expected 80 round constants, found {len(decs)}"
    rc_ext, rc_int = decs[:24], decs[24:]

    ext_rows = ",\n".join(
        "    [MontFp!(\"%s\"), MontFp!(\"%s\"), MontFp!(\"%s\")]" % tuple(rc_ext[i * 3 : i * 3 + 3])
        for i in range(8)
    )
    int_rows = ",\n".join(f'    MontFp!("{d}")' for d in rc_int)

    OUT.write_text(f'''\
//! Native Poseidon2 (BN254, t = 3, alpha = 5, R_F = 8, R_P = 56) — the exact
//! permutation the circuits use, for client-side hashing (commitments, Merkle
//! trees, nullifiers). GENERATED from `xark-poseidon2` by
//! `scripts/gen_poseidon2_native.py`; do not edit by hand. The
//! `matches_gadget_reference_vector` test pins it to the gadget's own KAT, so
//! the two implementations cannot silently drift.

use ark_bn254::Fr;
use ark_ff::{{AdditiveGroup, MontFp}};

/// External (full-round) constants: 8 rounds x 3 lanes.
const RC_EXT: [[Fr; 3]; 8] = [
{ext_rows},
];

/// Internal (partial-round) constants: 56 rounds, lane 0 only.
const RC_INT: [Fr; 56] = [
{int_rows},
];

/// S-box `x^5`.
fn sbox(x: Fr) -> Fr {{
    let x2 = x * x;
    x2 * x2 * x
}}

/// External linear layer `M_E = circ(2,1,1)`: `out[i] = state[i] + sum`.
fn matmul_external(s: [Fr; 3]) -> [Fr; 3] {{
    let sum = s[0] + s[1] + s[2];
    [s[0] + sum, s[1] + sum, s[2] + sum]
}}

/// Internal linear layer `M_I = [[2,1,1],[1,2,1],[1,1,3]]`: `out[i] = sum + diag[i]*s[i]`
/// with `diag = [1,1,2]`.
fn matmul_internal(s: [Fr; 3]) -> [Fr; 3] {{
    let sum = s[0] + s[1] + s[2];
    [sum + s[0], sum + s[1], sum + s[2].double()]
}}

fn external_round(s: [Fr; 3], rc: [Fr; 3]) -> [Fr; 3] {{
    matmul_external([sbox(s[0] + rc[0]), sbox(s[1] + rc[1]), sbox(s[2] + rc[2])])
}}

fn internal_round(mut s: [Fr; 3], rc0: Fr) -> [Fr; 3] {{
    s[0] = sbox(s[0] + rc0);
    matmul_internal(s)
}}

/// The Poseidon2 permutation on a width-3 BN254 state.
pub fn perm(state: [Fr; 3]) -> [Fr; 3] {{
    let mut s = matmul_external(state);
    for rc in &RC_EXT[..4] {{
        s = external_round(s, *rc);
    }}
    for rc in RC_INT {{
        s = internal_round(s, rc);
    }}
    for rc in &RC_EXT[4..] {{
        s = external_round(s, *rc);
    }}
    s
}}

/// 2-to-1 compression, mirroring the gadget: `hash2(a, b) = perm([a, b, 0])[0]`.
pub fn hash2(a: Fr, b: Fr) -> Fr {{
    perm([a, b, Fr::ZERO])[0]
}}

/// Variable-length sponge (rate 2, capacity 1), mirroring the gadget's
/// `hash::<N>`: the capacity lane is seeded with the length for domain
/// separation; inputs are absorbed two per permutation.
pub fn hash(inputs: &[Fr]) -> Fr {{
    let mut state = [Fr::ZERO, Fr::ZERO, Fr::from(inputs.len() as u64)];
    for pair in inputs.chunks(2) {{
        state[0] += pair[0];
        if let Some(second) = pair.get(1) {{
            state[1] += second;
        }}
        state = perm(state);
    }}
    state[0]
}}

#[cfg(test)]
mod tests {{
    use super::*;
    use std::str::FromStr;

    /// The gadget's own reference vector (`xark-poseidon2/tests/vec.rs`,
    /// produced by `poseidon2_ref.py` from the canonical Horizen Labs
    /// constants): `perm([1, 2, 3])`.
    #[test]
    fn matches_gadget_reference_vector() {{
        let out = perm([Fr::from(1u64), Fr::from(2u64), Fr::from(3u64)]);
        let expect = [
            "4737982494702600552753609419126955242994596445692557044681458296415162795880",
            "9698155156890762076414037574068404457164720954413259397447872502075783415658",
            "18259628997120261506554896720810362547891614655348127750921457211768261324825",
        ]
        .map(|d| Fr::from_str(d).unwrap());
        assert_eq!(out, expect);
    }}
}}
''')
    subprocess.run(["rustfmt", "--edition", "2021", str(OUT)], check=True)
    print(f"wrote {OUT} ({len(rc_ext)} ext + {len(rc_int)} int constants from {src_path})")


if __name__ == "__main__":
    main()
