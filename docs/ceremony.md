# From a single-party key to a ceremony

Every example builds with a **single-party key**: `xark setup`, with no `.ptau`, samples the
Groth16 trapdoor from one machine's randomness and never writes it down. The key is sound unless
whoever ran setup kept that randomness, but everyone relying on the verifier has to trust that one
party; the exported crate's `SETUP` constant says `single-party`. This page replaces the key with a
**multi-party ceremony**, using example 01's circuit, so no single party has to be trusted. Every command
below was run as written. Why a ceremony works: [Trusted setup, intuitively](./learn-zk/04-trusted-setup-intuition.md).

## 1. Phase 1: download a public transcript

Phase 1 is universal, the same for every circuit. Don't run your own: reuse a public one with many
contributors, such as the [Perpetual Powers of Tau](https://github.com/privacy-scaling-explorations/perpetualpowersoftau).
A transcript of size `2^k` covers circuits up to about `2^k` constraints; 01 has 128, so `k = 10`
is plenty (`xark ceremony init` says if a transcript is too small).

```bash
curl -fLO https://pse-trusted-setup-ppot.s3.eu-central-1.amazonaws.com/pot28_0080/ppot_0080_10.ptau
shasum -a 256 ppot_0080_10.ptau
# 75b53cf75b5a1068d3a19575bf8164231b8a4babbf15383490de9c0144a15bf8
```

Check the hash against the one the transcript's maintainers publish before you trust it.

## 2. Phase 2: the ceremony

Phase 2 is specific to your circuit, so it runs once per circuit, and again whenever the circuit
changes (its digest changes). Each contributor adds randomness and then destroys it. The keys are
safe if **any one** contributor was honest.

```bash
xark build circuit
xark ceremony init circuit --ptau-file ppot_0080_10.ptau --out ceremony
xark ceremony contribute --ceremony-dir ceremony --label alice   # on Alice's machine
xark ceremony contribute --ceremony-dir ceremony --label bob     # then on Bob's
xark ceremony verify circuit --ceremony-dir ceremony --ptau-file ppot_0080_10.ptau   # anyone can check the chain
xark ceremony finalize circuit --ceremony-dir ceremony --ptau-file ppot_0080_10.ptau
# verifying key sha256: 6915bf21…8750  (pin a client's verifier to it: { keyHash })
```

`ceremony/` now holds `proving_key.bin`, `verifying_key.bin`, one attestation per contribution, and a
`metadata.json` that records the ceremony and its contributors. `xark ceremony verify` recomputes the
initial keys from the transcript and checks every contribution against them. In a real ceremony the directory passes from
contributor to contributor, each on their own machine. Publish the attestations so anyone can rerun
`xark ceremony verify`.

## 3. Use the keys

```bash
xark prove circuit --inputs '{"power_level": 9001}' --proving-key ceremony/proving_key.bin --out proof
xark export circuit --verifying-key ceremony/verifying_key.bin \
  --proof proof/proof.bin --public-inputs proof/public_inputs.bin \
  --crate-name over-9000-xark-verifier
```

`xark export` reports "a 2-party ceremony", and the verifier crate's `SETUP` is `ceremony` (with one
contributor it would be `single-party`: one party's randomness again). Build the program against
that verifier crate as before.

In the app, serve `ceremony/proving_key.bin` as the prover's `pk.bin`, and pin both ends to what
you shipped:

```ts
const prover = await Over9000.prover("/circuits/over_9000/", { digest: Over9000.digest });
const verifier = await Over9000.verifier(vk, { keyHash: "6915bf21…8750" }); // the finalize line
```

`digest` refuses any other circuit, and `keyHash` any other verifying key, so a compromised server
can't swap either under your users.

## For a production ceremony

xark's [trusted-setup guide](https://github.com/blueshift-gg/xark/blob/master/docs/trusted-setup.md)
lists what a real one needs: a public phase-1 transcript finished with a randomness beacon, several
independent phase-2 contributors who each publish their attestation, and `xark ceremony verify` run by third parties. The commands are the ones above.
