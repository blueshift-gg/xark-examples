# Upstream snapshot

The source in `src/` is copied without modification from:

- Repository: `https://github.com/blueshift-gg/xark`
- Commit: `44f9a8d3b1f5af90a57249fa1dad308968922a03`
- Path: `crates/verifier/src/`

The standalone `Cargo.toml` changes one dependency edge:

```toml
solana-nostd-alt-bn128 = { version = "0.1.1", default-features = false }
```

The upstream manifest enables that crate's `static-syscalls` default. Programs built through the
current SBF toolchain fail LiteSVM loading with an out-of-bounds relative jump on that path. This
snapshot keeps the verifier implementation pinned while selecting dynamic syscall linkage. Remove
the snapshot and Cargo patches once upstream xark makes the dependency feature choice compatible.
