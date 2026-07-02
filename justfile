# Build every example end to end, then run the in-process (LiteSVM) test suite.
# Requires the toolchain in RUNBOOK.md (nargo 1.0.0-beta.22, xark, anza CLI).
#
# This is the orchestrator: each recipe delegates to the example's own justfile,
# which stays self-contained so it reads top-to-bottom on its own.

set shell := ["bash", "-uc"]

# One command: build all artifacts, then verify every proof in a Solana VM.
test: build-all
    cd e2e && cargo test

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

# Run every circuit's `nargo test` (fast, needs only nargo).
test-circuits:
    cd 01-over-9000/circuit && nargo test
    cd 02-age-verification/circuit && nargo test
    cd 03-shielded-pool/circuits/deposit && nargo test
    cd 03-shielded-pool/circuits/withdraw && nargo test
    cd 04-shielded-transfer/circuits/transact && nargo test

clean:
    cd 01-over-9000 && just clean
    cd 02-age-verification && just clean
    cd 03-shielded-pool && just clean
    cd 04-shielded-transfer && just clean
    rm -rf e2e/target
