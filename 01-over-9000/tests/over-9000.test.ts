// Prove "over 9000" with the generated client, then verify it onchain: the
// instruction data is the proof itself, 256 bytes.
import { generateKeyPairSigner, type Address, type Instruction } from "@solana/kit";
import { circuit, funded, program as deployed, send, vm } from "@blueshift-gg/xark/testing";
import { CheckFailed, Over9000, type Over9000Prover } from "../clients/over-9000/ts/over_9000";

let program: Address;
let prover: Over9000Prover;
beforeAll(async () => {
  program = (await generateKeyPairSigner()).address; // the program takes any address
  // Pinned to the circuit this client was generated from: a deployment can't swap it.
  prover = await Over9000.prover(circuit("over-9000"), { digest: Over9000.digest });
});

const verify = (data: Uint8Array): Instruction => ({ programAddress: program, data });

test("a proof of 9001 verifies onchain", async () => {
  const proof = prover.prove({ powerLevel: 9001 });
  const svm = vm({ [program]: deployed("over-9000") });
  const payer = await funded(svm);

  await send(svm, payer, verify(proof.toBytes()));

  const tampered = proof.toBytes();
  tampered[0] ^= 1;
  await expect(send(svm, payer, verify(tampered))).rejects.toThrow(/InvalidProof/);
  // Anything but the 256 bytes is not this instruction.
  await expect(send(svm, payer, verify(proof.toBytes().slice(1)))).rejects.toThrow(/invalid instruction data/);
});

test("9000 has no proof", () => {
  // The prover names the check that fails, from the circuit's source.
  expect(() => prover.prove({ powerLevel: 9000 })).toThrow(CheckFailed);
  expect(() => prover.prove({ powerLevel: 9000 })).toThrow(/power_level > 9000/);
});
