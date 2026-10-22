# Every example, end to end: build its circuits, keys, verifier crates, TypeScript
# clients and program, then prove with the clients and verify on-chain (LiteSVM).
# Prerequisites: RUNBOOK.md. Each example's own justfile reads top to bottom.

set shell := ["bash", "-euo", "pipefail", "-c"]

examples := "01-over-9000 02-age-verification 03-shielded-pool 04-shielded-transfer"

# Build everything, then run every example's client tests.
test: build
    [ -d node_modules ] || npm ci
    npm test

build:
    for e in {{examples}}; do just --justfile $e/justfile --working-directory $e build; done

# The circuits' own Rust tests.
test-circuits:
    for e in {{examples}}; do just --justfile $e/justfile --working-directory $e test-circuit; done

# The committed clients are the ones `just build` generates from the circuits.
check-clients: build
    git diff --exit-code -- '*/client/'
    test -z "$(git status --porcelain -- '*/client/')"

fmt-check:
    for m in $(git ls-files '*Cargo.toml'); do cargo fmt --check --manifest-path $m; done

# After `build`, which generates the verifier crates the programs depend on.
clippy:
    for m in $(git ls-files '*Cargo.toml'); do cargo clippy --manifest-path $m --workspace --all-targets --locked -- -D warnings; done

clean:
    for e in {{examples}}; do just --justfile $e/justfile --working-directory $e clean; done
