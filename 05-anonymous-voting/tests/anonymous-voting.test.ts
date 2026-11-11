// A poll from both sides: an authority publishes the root of its voter list and
// creates a poll; voters prove they're on the list, at a path the chain never sees,
// and vote once each, through a relayer that pays for them. The list's builder
// (`Identity.commitment`, `Path`) is the circuit's own Rust, run from TypeScript.
import { AccountRole, address, getAddressEncoder, getProgramDerivedAddress, type Address, type KeyPairSigner } from "@solana/kit";
import type { LiteSVM } from "litesvm";
import { circuit, funded, program as deployed, send, vm } from "@blueshift-gg/xark/testing";
import { Ballot, Field, Identity, Path, Vote, type PathValue, type VoteProver } from "../clients/vote/ts/vote";
import { u64le } from "./helpers";

const program = address("HrQMmAAVGe9MhJMXMjQYsNA3g9qztY3TMjG9d4gKT1r6");
const system = address("11111111111111111111111111111111");
const bytes = (a: Address) => new Uint8Array(getAddressEncoder().encode(a));
// The program's instruction discriminators (`programs/anonymous-voting/src/instructions/`).
const CREATE_POLL = 0;
const VOTE = 1;
const CLOSE_POLL = 2;
const signer = (s: KeyPairSigner) => ({ address: s.address, role: AccountRole.WRITABLE_SIGNER, signer: s });
const writable = (a: Address) => ({ address: a, role: AccountRole.WRITABLE });
const readonly = (a: Address) => ({ address: a, role: AccountRole.READONLY });

let prover: VoteProver;
beforeAll(async () => {
  // Pinned to the circuit this client was generated from: a deployment can't swap it.
  prover = await Vote.prover(circuit("vote"), { digest: Vote.digest });
});

/** A new voter: two random field elements, kept by the voter. */
const newIdentity = (): Identity => ({
  nullifierKey: new Field(crypto.getRandomValues(new Uint8Array(31))),
  trapdoor: new Field(crypto.getRandomValues(new Uint8Array(31))),
});

/**
 * The authority's voter list: its root, and each voter's path. Each commitment is
 * added at the next free leaf, and every path so far updated beside it, with the
 * circuit's own `Path`. (For a big list the authority publishes each path; a voter
 * can check theirs with `path.root(Identity.commitment(identity))`.)
 */
function list(voters: Identity[]): { root: Field; paths: PathValue[] } {
  let tip = Path.first(); // the next free leaf's path
  let paths: PathValue[] = [];
  for (const voter of voters) {
    const leaf = Identity.commitment(voter);
    paths = paths.map((p) => p.update(tip, leaf));
    paths.push(tip);
    tip = tip.next(leaf);
  }
  return { root: tip.listRoot(), paths };
}

/** Create poll `id` over the list rooted at `root`, with `options` options. */
async function createPoll(svm: LiteSVM, authority: KeyPairSigner, id: bigint, root: Field, options: number): Promise<Address> {
  const [poll] = await getProgramDerivedAddress({ programAddress: program, seeds: ["poll", u64le(id)] });
  await send(svm, authority, {
    programAddress: program,
    accounts: [signer(authority), writable(poll), readonly(system)],
    data: new Uint8Array([CREATE_POLL, ...u64le(id), ...root.toBytes(), options]),
  });
  return poll;
}

/** A vote for `choice` in `poll`, with the proof and nullifier, paid by `payer`. */
async function vote(poll: Address, payer: KeyPairSigner, proof: Uint8Array, nullifier: Field, choice: number) {
  const [receipt] = await getProgramDerivedAddress({ programAddress: program, seeds: ["voted", bytes(poll), nullifier.toBytes()] });
  return {
    programAddress: program,
    accounts: [signer(payer), writable(poll), writable(receipt), readonly(system)],
    data: new Uint8Array([VOTE, ...proof, ...nullifier.toBytes(), choice]),
  };
}

/** The poll's tally, one count per option (`programs/anonymous-voting/src/state.rs`: from byte 80). */
function tally(svm: LiteSVM, poll: Address, options: number): bigint[] {
  const account = svm.getAccount(poll);
  if (!account.exists) throw new Error("no poll");
  const view = new DataView(account.data.buffer, account.data.byteOffset);
  return Array.from({ length: options }, (_, i) => view.getBigUint64(80 + 8 * i, true));
}

test("each voter on the list votes once per poll, and nobody sees who", async () => {
  const svm = vm({ [program]: deployed("anonymous-voting") });
  const authority = await funded(svm);
  // Any wallet pays for a vote: a relayer, so no voter's wallet appears onchain.
  const relayer = await funded(svm);

  // Five voters register their commitments; the authority publishes the list's root.
  const voters = Array.from({ length: 5 }, newIdentity);
  const { root, paths } = list(voters);
  const poll = await createPoll(svm, authority, 1n, root, 3);

  // A voter's ballot and its proof, at their path in the list.
  const ballot = (i: number, pollId: bigint, choice: number, options = 3) => {
    const b = Ballot.of(voters[i], paths[i], pollId, options, choice);
    const start = performance.now();
    const proof = prover.prove({ identity: voters[i], path: paths[i], ballot: b }).toBytes();
    console.log(`vote: proved in ${Math.round(performance.now() - start)} ms`);
    return { proof, nullifier: b.nullifier };
  };

  // Three of them vote: 0 for option 0, 1 and 2 for option 2.
  for (const [i, choice] of [[0, 0], [1, 2], [2, 2]]) {
    const { proof, nullifier } = ballot(i, 1n, choice);
    const result = await send(svm, relayer, await vote(poll, relayer, proof, nullifier, choice));
    console.log(`vote: a vote uses ${result.computeUnitsConsumed()} compute units`);
  }
  expect(tally(svm, poll, 3)).toEqual([1n, 0n, 2n]);

  // Voter 0 again, with a fresh proof: the same nullifier, whose receipt exists.
  const again = ballot(0, 1n, 1);
  await expect(send(svm, relayer, await vote(poll, relayer, again.proof, again.nullifier, 1))).rejects.toThrow(/AlreadyVoted/);

  // Voter 3's proof for option 1, replayed for another choice, in range or not: the
  // program fills `choice` in, and the proof fails.
  const three = ballot(3, 1n, 1);
  for (const choice of [0, 2, 3]) {
    await expect(send(svm, relayer, await vote(poll, relayer, three.proof, three.nullifier, choice))).rejects.toThrow(/InvalidProof/);
  }
  // No proof of a choice past the options: the circuit's check fails in the client.
  expect(() => ballot(3, 1n, 3)).toThrow(/ballot\.choice < ballot\.options/);
  // A proof made against more options than the poll has: the program fills in 3.
  const four = ballot(3, 1n, 3, 4);
  await expect(send(svm, relayer, await vote(poll, relayer, four.proof, four.nullifier, 3))).rejects.toThrow(/InvalidProof/);

  // A second poll over the same list. Voter 3's proof for poll 1 doesn't count there.
  const second = await createPoll(svm, authority, 2n, root, 3);
  await expect(send(svm, relayer, await vote(second, relayer, three.proof, three.nullifier, 1))).rejects.toThrow(/InvalidProof/);

  // Voter 0 votes in it: another poll, another nullifier, unrelated to the first.
  const other = ballot(0, 2n, 1);
  expect(other.nullifier.toBytes()).not.toEqual(again.nullifier.toBytes());
  await send(svm, relayer, await vote(second, relayer, other.proof, other.nullifier, 1));
  expect(tally(svm, second, 3)).toEqual([0n, 1n, 0n]);

  // Voter 3 casts the proof made for poll 1 in poll 1, and the tally is final.
  await send(svm, relayer, await vote(poll, relayer, three.proof, three.nullifier, 1));
  expect(tally(svm, poll, 3)).toEqual([1n, 1n, 2n]);

  // The authority closes the second poll: nothing counts after that.
  await send(svm, authority, { programAddress: program, accounts: [signer(authority), writable(second)], data: new Uint8Array([CLOSE_POLL]) });
  const late = ballot(4, 2n, 0);
  await expect(send(svm, relayer, await vote(second, relayer, late.proof, late.nullifier, 0))).rejects.toThrow(/PollClosed/);
  expect(tally(svm, second, 3)).toEqual([0n, 1n, 0n]);
});

test("an identity not on the list has no proof", async () => {
  const voters = [newIdentity(), newIdentity()];
  const { root, paths } = list(voters);
  const stranger = newIdentity();
  // At a voter's path, or the next free one: no leaf of the list is the stranger's.
  for (const path of [paths[0], paths[1].next(Identity.commitment(voters[1]))]) {
    const ballot = { ...Ballot.of(stranger, path, 1n, 3, 0), root };
    expect(() => prover.prove({ identity: stranger, path, ballot })).toThrow(/path\.root\(identity\.commitment\(\)\)/);
  }
});
