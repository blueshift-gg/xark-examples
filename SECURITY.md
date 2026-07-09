# Security policy

## Project status

This repository contains educational reference implementations. They exercise real Groth16 proofs
and Solana programs, but they are not audited deployments and the default recipes generate
development setup keys. Do not use generated keys, fixture secrets, program IDs, or wallet data with
assets of value.

## Reporting a vulnerability

Do not open a public issue for a vulnerability that could affect proof soundness, key material,
funds, privacy, or the xark verifier integration. Use GitHub's **Security -> Report a vulnerability**
flow for this repository so maintainers can investigate privately.

Include the affected example and commit, the violated invariant, the smallest useful reproduction,
and whether the issue is in this repository or the upstream xark toolchain. Do not include real user
secrets or mainnet key material.

General documentation bugs and non-sensitive build failures can use normal GitHub issues.

## Supported version

Only the current default branch is maintained. Dependency and verifier revisions are intentionally
pinned; reports against a locally modified or floating xark revision should first be reproduced
against the revision in `RUNBOOK.md`.
