// An issuer commits to a holder's identity; the holder proves it meets the program's
// policy with the generated client; the program checks the proof against the
// commitment, its own policy on today's date, and the signer.
import { AccountRole, address, generateKeyPairSigner, type Address, type KeyPairSigner } from "@solana/kit";
import type { LiteSVM } from "litesvm";
import { circuit, funded, program as deployed, send, vm } from "@blueshift-gg/xark/testing";
import { CheckFailed, Country, Field, Identity, Policy, Zkyc, type ZkycProver } from "../clients/zkyc/ts/zkyc";

let program: Address;
let prover: ZkycProver;
beforeAll(async () => {
  program = (await generateKeyPairSigner()).address; // the program takes any address
  // From the build output, the prover also loads `client.wasm`: the circuit's Rust
  // functions, such as `Identity.commitment` and `Policy.default`.
  // Pinned to the circuit this client was generated from: a deployment can't swap it.
  prover = await Zkyc.prover(circuit("zkyc"), { digest: Zkyc.digest });
});

/** The holder's credential, from the issuer: the salt is the issuer's secret. */
const credential = (changes: Partial<Identity> = {}): Identity => ({
  document: "A1938274",
  born: new Date("1990-01-01"),
  nationality: Country.US,
  netWorth: 1_000_000,
  expires: new Date("2030-01-01"),
  salt: new Field(crypto.getRandomValues(new Uint8Array(31))),
  ...changes,
});

/**
 * The program's policy on `today` (its constants in `programs/zkyc/src/policy.rs`), built
 * with the circuit's Rust: 18 or older, worth at least 500,000, a national of DE, FR,
 * GB or US, with a document valid that day.
 */
const programs = (today: string) =>
  Policy.default().on(new Date(today)).minAge(18).worthAtLeast(500_000).nationalities([Country.DE, Country.FR, Country.GB, Country.US]);

/** The instruction: signed by `holder`, its discriminator (0), the proof, then the commitment it's for. */
const verifyKyc = (holder: KeyPairSigner, proof: Uint8Array, commitment: Field) => ({
  programAddress: program,
  accounts: [{ address: holder.address, role: AccountRole.READONLY_SIGNER, signer: holder }],
  data: new Uint8Array([0, ...proof, ...commitment.toBytes()]),
});

/** Set the VM's clock to noon UTC on `day`. */
function setDay(svm: LiteSVM, day: string) {
  const clock = svm.getClock();
  clock.unixTimestamp = BigInt(Date.parse(`${day}T12:00:00Z`) / 1000);
  svm.setClock(clock);
}

test("a compliant holder passes KYC onchain, under the program's policy and for their wallet only", async () => {
  const id = credential();
  // The issuer publishes this; it's the circuit's own Rust, `Identity::commitment`.
  const commitment = Identity.commitment(id);
  const svm = vm({ [program]: deployed("zkyc") });
  const payer = await funded(svm);
  const holder = await generateKeyPairSigner(); // the holder's wallet: it signs, payer pays
  setDay(svm, "2026-10-22");

  const policy = programs("2026-10-22");
  expect(policy.bornBy).toEqual(new Date("2008-10-22"));
  const proof = prover.prove({ id, commitment, policy, holder: holder.address });
  const ok = await send(svm, payer, verifyKyc(holder, proof.toBytes(), commitment));
  expect(ok.computeUnitsConsumed()).toBeLessThan(200_000n);

  // Someone who saw the proof can't submit it as their own.
  const copycat = await generateKeyPairSigner();
  await expect(send(svm, payer, verifyKyc(copycat, proof.toBytes(), commitment))).rejects.toThrow(/InvalidProof/);

  // Another commitment: someone else's credential.
  const other = Identity.commitment(credential());
  await expect(send(svm, payer, verifyKyc(holder, proof.toBytes(), other))).rejects.toThrow(/InvalidProof/);

  // Tomorrow the program supplies another `today` and `born_by`, so yesterday's proof fails.
  setDay(svm, "2026-10-23");
  await expect(send(svm, payer, verifyKyc(holder, proof.toBytes(), commitment))).rejects.toThrow(/InvalidProof/);
});

test("the verifier picks the rules, not the prover", async () => {
  // A holder too poor for the program's policy proves against a laxer one of their
  // own: a valid proof, of the wrong statement. The program supplies its policy.
  const id = credential({ netWorth: 100 });
  const commitment = Identity.commitment(id);
  const svm = vm({ [program]: deployed("zkyc") });
  const payer = await funded(svm);
  const holder = await generateKeyPairSigner();
  setDay(svm, "2026-10-22");

  const lax = programs("2026-10-22").worthAtLeast(0);
  const proof = prover.prove({ id, commitment, policy: lax, holder: holder.address });
  await expect(send(svm, payer, verifyKyc(holder, proof.toBytes(), commitment))).rejects.toThrow(/InvalidProof/);
});

test("an identity that fails the policy has no proof", () => {
  const policy = programs("2026-10-22");
  const prove = (changes: Partial<Identity>) => () => {
    const id = credential(changes);
    prover.prove({ id, commitment: Identity.commitment(id), policy, holder: address("11111111111111111111111111111111") });
  };
  // The client names the check that fails, from the circuit's source.
  expect(prove({ born: new Date("2008-10-23") })).toThrow(CheckFailed); // 18 tomorrow
  expect(prove({ born: new Date("2008-10-23") })).toThrow(/`require\(id\.born <= policy\.born_by\);`/);
  expect(prove({ netWorth: 499_999 })).toThrow(/`require\(id\.net_worth >= policy\.min_net_worth\);`/);
  expect(prove({ nationality: Country.JP })).toThrow(/`id\.nationality\.require_in\(policy\.allowed\);`/);
  expect(prove({ expires: new Date("2026-10-21") })).toThrow(/`require\(id\.expires >= policy\.today\);`/);
  // Every limit is inclusive: 18 today, exactly 500,000, the document's last day.
  prove({ born: new Date("2008-10-22"), netWorth: 500_000, expires: new Date("2026-10-22"), nationality: Country.GB })();
});
