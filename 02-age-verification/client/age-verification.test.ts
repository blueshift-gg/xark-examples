// An issuer commits to a holder's birth date; the holder proves they're 18 or older
// today with the generated client; the program checks the proof against the
// commitment and its own clock.
import { AccountRole, generateKeyPairSigner, getAddressEncoder, type Address, type KeyPairSigner } from "@solana/kit";
import type { LiteSVM } from "litesvm";
import { artifacts, funded, send, vm } from "./harness";
import { AgeVerification, CheckFailed, Field, Identity, Policy, type AgeVerificationProver } from "./age_verification";

let program: Address;
let prover: AgeVerificationProver;
beforeAll(async () => {
  program = (await generateKeyPairSigner()).address; // the program takes any address
  // From the build output, the prover also loads `client.wasm`: the circuit's Rust
  // functions, such as `Identity.commitment`.
  // Pinned to the circuit this client was generated from: a deployment can't swap it.
  prover = await AgeVerification.prover(artifacts("circuit", "age_verification"), { digest: AgeVerification.digest });
});

/** The holder's credential, from the issuer: the salt is the issuer's secret. */
const credential = (born: string): Identity => ({ born: new Date(born), salt: new Field(crypto.getRandomValues(new Uint8Array(31))) });

/** The instruction: signed by `holder`, the proof, then the commitment it's for. */
const verifyAge = (holder: KeyPairSigner, proof: Uint8Array, commitment: Field) => ({
  programAddress: program,
  accounts: [{ address: holder.address, role: AccountRole.READONLY_SIGNER, signer: holder }],
  data: new Uint8Array([...proof, ...commitment.toBytes()]),
});
const bytes = (a: Address) => new Uint8Array(getAddressEncoder().encode(a));

/** Set the VM's clock to noon UTC on `day`. */
function setDay(svm: LiteSVM, day: string) {
  const clock = svm.getClock();
  clock.unixTimestamp = BigInt(Date.parse(`${day}T12:00:00Z`) / 1000);
  svm.setClock(clock);
}

test("an adult proves their age on-chain, for today and their wallet only", async () => {
  const id = credential("1990-01-01");
  // The issuer publishes this; it's the circuit's own Rust, `Identity::commitment`.
  const commitment = Identity.commitment(id);
  const svm = vm(program, "program/target/deploy/age_verification_program.so");
  const payer = await funded(svm);
  const holder = await generateKeyPairSigner(); // the holder's wallet: it signs, payer pays
  setDay(svm, "2026-10-22");

  // The program's own rule, from the circuit's Rust: 18 or older today.
  const policy = Policy.adultOn(new Date("2026-10-22"));
  expect(policy.bornBy).toEqual(new Date("2008-10-22"));
  const proof = prover.prove({ id, commitment, policy, holder: bytes(holder.address) });
  await send(svm, payer, verifyAge(holder, proof.toBytes(), commitment));

  // Someone who saw the proof can't submit it as their own.
  const copycat = await generateKeyPairSigner();
  await expect(send(svm, payer, verifyAge(copycat, proof.toBytes(), commitment))).rejects.toThrow(/invalid instruction data/);

  // Another commitment: someone else's credential.
  const other = Identity.commitment(credential("1990-01-01"));
  await expect(send(svm, payer, verifyAge(holder, proof.toBytes(), other))).rejects.toThrow(/invalid instruction data/);

  // Tomorrow the program supplies another `today`, so yesterday's proof fails.
  setDay(svm, "2026-10-23");
  await expect(send(svm, payer, verifyAge(holder, proof.toBytes(), commitment))).rejects.toThrow(/invalid instruction data/);
});

test("a minor has no proof", () => {
  const id = credential("2008-10-23"); // 18 tomorrow
  const policy = Policy.adultOn(new Date("2026-10-22"));
  const prove = () => prover.prove({ id, commitment: Identity.commitment(id), policy, holder: new Uint8Array(32) });
  expect(prove).toThrow(CheckFailed);
  expect(prove).toThrow(/id\.born <= policy\.born_by/);
});
