#!/usr/bin/env python3
"""Generate the deposit -> transfer -> withdraw proof chain for the e2e.

Run from 04-shielded-transfer/circuits/transact (see ../../justfile `gen-chain`)
AFTER `xark setup`. Re-runs the in-circuit `print_chain` witness simulator,
assembles a Prover.toml per transaction, proves each (reusing the proving key so
all three share the program's embedded verifying key), and writes the proof +
public-input bytes to ./chain/{dep,xfer,wd}_{proof,pi}.bin.
"""
import os, shutil, subprocess

ASSET = '"246862576160547569215256475045468302459"'
EMPTY_ROOT = '"0x0603ce87c578971991473e47c83619f7fee7c68e4caf8a4fe54b25be100ca04b"'
Z20 = "[" + ",".join(['"0"'] * 20) + "]"

env = dict(os.environ)
env["PATH"] = (
    os.path.expanduser("~/.cargo/bin") + ":" + os.path.expanduser("~/.nargo/bin") + ":" + env["PATH"]
)


def run(cmd):
    r = subprocess.run(cmd, capture_output=True, text=True, env=env)
    if r.returncode != 0:
        raise SystemExit(f"{cmd} failed:\n{r.stdout[-800:]}\n{r.stderr[-800:]}")
    return r.stdout


def parse(out):
    lines = [l.strip() for l in out.splitlines()]
    labels = {
        "ADDRA", "ADDRB",
        "DEP nf0", "DEP nf1", "DEP cm0", "DEP cm1", "DEP new_root",
        "XFER path0", "XFER index0", "XFER nf0", "XFER nf1", "XFER cm0", "XFER cm1",
        "XFER filled", "XFER old_root(root1)", "XFER new_root(root2)",
        "WD path0", "WD index0", "WD nf0", "WD nf1", "WD cm0", "WD cm1",
        "WD filled", "WD old_root(root2)", "WD new_root(root3)",
    }
    vals = {}
    for i, l in enumerate(lines):
        if l in labels:
            j = i + 1
            while j < len(lines) and lines[j] == "":
                j += 1
            vals[l] = lines[j]
    return vals


def q(v):
    return '"' + v + '"'


def arr(s):
    inner = s.strip()[1:-1]
    return "[" + ",".join('"' + x.strip() + '"' for x in inner.split(",")) + "]"


v = parse(run(["nargo", "test", "--show-output", "print_chain"]))
A, B = q(v["ADDRA"]), q(v["ADDRB"])

tomls = {
    "dep": f'''sk="42"
in_value=["0","0"]
in_d=["7","7"]
in_rho=["101","107"]
in_rseed=["103","109"]
in_path=[{Z20},{Z20}]
in_index=[{Z20},{Z20}]
in_enforce=["0","0"]
out_value=["150","0"]
out_addr=[{A},"888"]
out_rho=["11","27"]
out_rseed=["13","29"]
filled_subtrees={Z20}
root={EMPTY_ROOT}
asset={ASSET}
nf=[{q(v["DEP nf0"])},{q(v["DEP nf1"])}]
cm_out=[{q(v["DEP cm0"])},{q(v["DEP cm1"])}]
old_root={EMPTY_ROOT}
new_root={q(v["DEP new_root"])}
insert_index="0"
vpub_in="150"
vpub_out="0"
fee="0"
''',
    "xfer": f'''sk="42"
in_value=["150","0"]
in_d=["7","7"]
in_rho=["11","201"]
in_rseed=["13","203"]
in_path=[{arr(v["XFER path0"])},{Z20}]
in_index=[{arr(v["XFER index0"])},{Z20}]
in_enforce=["1","0"]
out_value=["100","50"]
out_addr=[{B},{A}]
out_rho=["31","41"]
out_rseed=["33","43"]
filled_subtrees={arr(v["XFER filled"])}
root={q(v["XFER old_root(root1)"])}
asset={ASSET}
nf=[{q(v["XFER nf0"])},{q(v["XFER nf1"])}]
cm_out=[{q(v["XFER cm0"])},{q(v["XFER cm1"])}]
old_root={q(v["XFER old_root(root1)"])}
new_root={q(v["XFER new_root(root2)"])}
insert_index="2"
vpub_in="0"
vpub_out="0"
fee="0"
''',
    "wd": f'''sk="99"
in_value=["100","0"]
in_d=["5","5"]
in_rho=["31","301"]
in_rseed=["33","303"]
in_path=[{arr(v["WD path0"])},{Z20}]
in_index=[{arr(v["WD index0"])},{Z20}]
in_enforce=["1","0"]
out_value=["0","0"]
out_addr=["888","888"]
out_rho=["51","61"]
out_rseed=["53","63"]
filled_subtrees={arr(v["WD filled"])}
root={q(v["WD old_root(root2)"])}
asset={ASSET}
nf=[{q(v["WD nf0"])},{q(v["WD nf1"])}]
cm_out=[{q(v["WD cm0"])},{q(v["WD cm1"])}]
old_root={q(v["WD old_root(root2)"])}
new_root={q(v["WD new_root(root3)"])}
insert_index="4"
vpub_in="0"
vpub_out="100"
fee="0"
''',
}

os.makedirs("chain", exist_ok=True)
vd = "target/shielded_transact-xark-verifier"
for name, toml in tomls.items():
    open("Prover.toml", "w").write(toml)
    run(["nargo", "execute"])
    run(["xark", "prove"])
    run(["xark", "export", "--crate-name", "shielded-transfer-transact-xark-verifier"])
    shutil.copy(f"{vd}/proof.solana.bin", f"chain/{name}_proof.bin")
    shutil.copy(f"{vd}/public_inputs.solana.bin", f"chain/{name}_pi.bin")
    print(f"{name}: proof + public inputs written to chain/")
