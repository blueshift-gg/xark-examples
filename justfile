# Build every example end to end, then run the in-process (LiteSVM) test suite.
# Requires the toolchain in RUNBOOK.md (xark + its pinned nightly, anza CLI).
#
# This is the orchestrator: each recipe delegates to the example's own justfile,
# which stays self-contained so it reads top-to-bottom on its own.

set shell := ["bash", "-uc"]

# One command: build all artifacts, then verify every proof in a Solana VM.
test: build-all
    cd e2e && cargo test --locked

# Build all four examples (circuits → proofs → verifier crates → programs).
build-all:
    cd 01-over-9000 && just build-program
    cd 02-age-verification && just build-program
    cd 03-shielded-pool && just prove-deposit
    cd 03-shielded-pool && just prove-withdraw
    cd 03-shielded-pool && just build-program
    cd 04-shielded-transfer && just setup
    cd 04-shielded-transfer && just gen-chain
    cd 04-shielded-transfer && just build-program

# Run the circuits that define native unit cases. The protocol-sized 03/04
# relations are exercised with real generated witnesses and Groth16 proofs by
# `just test`; invoking `xark test` there would only report "0 tests".
test-circuits:
    xark test 01-over-9000/circuit
    xark test 02-age-verification/circuit

# Validate every circuit against xark's supported Rust subset.
check-circuits:
    xark check 01-over-9000/circuit
    xark check 02-age-verification/circuit
    xark check 03-shielded-pool/circuits/deposit
    xark check 03-shielded-pool/circuits/withdraw
    xark check 04-shielded-transfer/circuits/transact

# One formatting gate for all standalone crates/workspaces.
fmt-check:
    cargo fmt --check --manifest-path 01-over-9000/circuit/Cargo.toml
    cargo fmt --check --manifest-path 01-over-9000/program/Cargo.toml
    cargo fmt --check --manifest-path 02-age-verification/circuit/Cargo.toml
    cargo fmt --check --manifest-path 02-age-verification/program/Cargo.toml
    cargo fmt --check --manifest-path 03-shielded-pool/circuits/deposit/Cargo.toml
    cargo fmt --check --manifest-path 03-shielded-pool/circuits/merkle/Cargo.toml
    cargo fmt --check --manifest-path 03-shielded-pool/circuits/withdraw/Cargo.toml
    cargo fmt --check --manifest-path 03-shielded-pool/program/Cargo.toml
    cargo fmt --check --manifest-path 04-shielded-transfer/circuits/transact/Cargo.toml
    cargo fmt --check --manifest-path 04-shielded-transfer/program/Cargo.toml
    cargo fmt --check --manifest-path e2e/Cargo.toml

# Run after `build-all`, which generates the verifier crates used by programs.
clippy:
    cargo clippy --manifest-path 01-over-9000/circuit/Cargo.toml --all-targets --locked -- -D warnings
    cargo clippy --manifest-path 01-over-9000/program/Cargo.toml --all-targets --locked -- -D warnings
    cargo clippy --manifest-path 02-age-verification/circuit/Cargo.toml --all-targets --locked -- -D warnings
    cargo clippy --manifest-path 02-age-verification/program/Cargo.toml --all-targets --locked -- -D warnings
    cargo clippy --manifest-path 03-shielded-pool/circuits/deposit/Cargo.toml --all-targets --locked -- -D warnings
    cargo clippy --manifest-path 03-shielded-pool/circuits/merkle/Cargo.toml --all-targets --locked -- -D warnings
    cargo clippy --manifest-path 03-shielded-pool/circuits/withdraw/Cargo.toml --all-targets --locked -- -D warnings
    cargo clippy --manifest-path 03-shielded-pool/program/Cargo.toml --workspace --all-targets --locked -- -D warnings
    cargo clippy --manifest-path 04-shielded-transfer/circuits/transact/Cargo.toml --all-targets --locked -- -D warnings
    cargo clippy --manifest-path 04-shielded-transfer/program/Cargo.toml --workspace --all-targets --locked -- -D warnings
    cargo clippy --manifest-path e2e/Cargo.toml --all-targets --locked -- -D warnings

clean:
    cd 01-over-9000 && just clean
    cd 02-age-verification && just clean
    cd 03-shielded-pool && just clean
    cd 04-shielded-transfer && just clean
    rm -rf e2e/target
