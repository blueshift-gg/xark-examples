# 002 · Full-pipeline CI

## Goal

CI that enforces the README's "built, proved, and verified end-to-end" claim on
every push — not just the circuit-test job that ships today (`.github/workflows/ci.yml`).

## What's already there

The `circuits` job installs nargo 1.0.0-beta.22 and runs `nargo test` on all four
circuits. That needs no xark and no Solana toolchain, so it runs today.

## What's missing (and the blocker)

The full pipeline — `just build-all && cd e2e && cargo test` — additionally needs:

- the **xark CLI**, which is currently a private, sibling-path dependency
  (`[patch.crates-io] xark-verifier = { path = "../../../xark/crates/verifier" }`), and
- the **Anza CLI** for `cargo build-sbf`.

The blocker is exposing xark to a runner. Options:

1. **Publish** `xark` + `xark-verifier` to crates.io → drop the patch, CI just
   `cargo install xark`. Cleanest; gated on the xark team publishing.
2. **Git submodule** of the (private) xark repo → CI needs a deploy key / token.
3. **Vendor** a pinned prebuilt `xark` binary + the `xark-verifier` source.

## Approach (once an option is chosen)

- New job: install noirup@beta.22 + Anza CLI + xark (per chosen option), cache
  `~/.cargo` and `target/`, run `just build-all` then `cd e2e && cargo test`.
- Add build-passing + license badges to the README.

## Open questions

- Which xark-exposure option (drives everything else).
- Runner time/cost: the arkworks build is minutes; caching matters.
- Pin the beta toolchains and plan for their bumps (noir beta.22 → next).

## Effort / risk

M once the xark-access decision is made. Risk: toolchain-install flakiness;
maintenance of pinned betas.

## Done when

A green CI job builds all three examples and runs the LiteSVM suite on every push.
