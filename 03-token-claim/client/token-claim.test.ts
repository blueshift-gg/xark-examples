// An airdrop from both sides: a distributor builds the list's root and funds a
// vault; a claimant proves they're on the list, at a path the chain never sees,
// and claims their amount once. The list's builder (`Claim.leaf`, `Path`) is the
// circuit's own Rust, run from TypeScript.
import {
  AccountRole,
  address,
  generateKeyPairSigner,
  getAddressEncoder,
  getProgramDerivedAddress,
  lamports,
  type Address,
  type KeyPairSigner,
} from "@solana/kit";
import { AccountState, TOKEN_PROGRAM_ADDRESS, getMintEncoder, getMintSize, getTokenDecoder, getTokenEncoder, getTokenSize } from "@solana-program/token";
import type { LiteSVM } from "litesvm";
import { artifacts, funded, send, u64le, vm } from "./harness";
import { Claim, Path, TokenClaim, type Field, type PathValue, type TokenClaimProver } from "./token_claim";

const system = address("11111111111111111111111111111111");
const bytes = (a: Address) => new Uint8Array(getAddressEncoder().encode(a));
const signer = (s: KeyPairSigner) => ({ address: s.address, role: AccountRole.WRITABLE_SIGNER, signer: s });
const writable = (a: Address) => ({ address: a, role: AccountRole.WRITABLE });
const readonly = (a: Address) => ({ address: a, role: AccountRole.READONLY });

const program = address("ETPfSrNBJ6Z29DQ7HzcqHtLcZLWCvS5Ar8VdnoByLZpX"); // the program's `ID`
let prover: TokenClaimProver;
beforeAll(async () => {
  // Pinned to the circuit this client was generated from: a deployment can't swap it.
  prover = await TokenClaim.prover(artifacts("circuit", "token_claim"), { digest: TokenClaim.digest });
});

/**
 * The distributor's list: its root, and each claim's path. Each claim is added at
 * the next free leaf, and every path so far updated beside it, with the circuit's
 * own `Path`. (For a big list the distributor publishes each path; a claimant can
 * check theirs with `path.root(Claim.leaf(claim))`.)
 */
function list(claims: Claim[]): { root: Field; paths: PathValue[] } {
  let tip = Path.first(); // the next free leaf's path
  let paths: PathValue[] = [];
  for (const claim of claims) {
    const leaf = Claim.leaf(claim);
    paths = paths.map((p) => p.update(tip, leaf));
    paths.push(tip);
    tip = tip.next(leaf);
  }
  return { root: tip.listRoot(), paths };
}

/** An SPL token account of `mint` for `owner`, holding `amount`. */
async function tokenAccount(svm: LiteSVM, mint: Address, owner: Address, amount: bigint): Promise<Address> {
  const at = (await generateKeyPairSigner()).address;
  const data = getTokenEncoder().encode({ mint, owner, amount, delegate: null, state: AccountState.Initialized, isNative: null, delegatedAmount: 0, closeAuthority: null });
  svm.setAccount({ address: at, data: new Uint8Array(data), executable: false, lamports: lamports(svm.minimumBalanceForRentExemption(BigInt(getTokenSize()))), programAddress: TOKEN_PROGRAM_ADDRESS, space: BigInt(getTokenSize()) });
  return at;
}
const balance = (svm: LiteSVM, at: Address) => {
  const account = svm.getAccount(at);
  return account.exists ? getTokenDecoder().decode(account.data).amount : 0n;
};

/** A VM with the program and a fresh mint, its supply in the distributor's token account. */
async function chain() {
  const svm = vm(program, "program/target/deploy/token_claim_program.so");
  const authority = await funded(svm);
  const mint = (await generateKeyPairSigner()).address;
  const mintData = getMintEncoder().encode({ mintAuthority: authority.address, supply: 10_000n, decimals: 0, isInitialized: true, freezeAuthority: null });
  svm.setAccount({ address: mint, data: new Uint8Array(mintData), executable: false, lamports: lamports(svm.minimumBalanceForRentExemption(BigInt(getMintSize()))), programAddress: TOKEN_PROGRAM_ADDRESS, space: BigInt(getMintSize()) });
  const supply = await tokenAccount(svm, mint, authority.address, 10_000n);
  return { svm, authority, mint, supply };
}
type Chain = Awaited<ReturnType<typeof chain>>;

/** A distributor for the list rooted at `root`, its vault funded with `total`. */
async function distribute(c: Chain, root: Field, total: bigint) {
  const [distributor] = await getProgramDerivedAddress({ programAddress: program, seeds: ["distributor", root.toBytes()] });
  const [vault] = await getProgramDerivedAddress({ programAddress: program, seeds: ["vault", bytes(distributor)] });
  await send(c.svm, c.authority, {
    programAddress: program,
    accounts: [signer(c.authority), writable(distributor), writable(vault), readonly(c.mint), writable(c.supply), readonly(TOKEN_PROGRAM_ADDRESS), readonly(system)],
    data: new Uint8Array([0, ...root.toBytes(), ...u64le(total)]),
  });
  return { distributor, vault };
}

/** A claim of `amount` from `distributor`, signed by `claimant`, paid to `to`. */
async function claim(distributor: { distributor: Address; vault: Address }, claimant: KeyPairSigner, to: Address, proof: Uint8Array, amount: bigint) {
  const [receipt] = await getProgramDerivedAddress({ programAddress: program, seeds: ["claimed", bytes(distributor.distributor), bytes(claimant.address)] });
  return {
    programAddress: program,
    accounts: [signer(claimant), readonly(distributor.distributor), writable(distributor.vault), writable(receipt), writable(to), readonly(TOKEN_PROGRAM_ADDRESS), readonly(system)],
    data: new Uint8Array([1, ...proof, ...u64le(amount)]),
  };
}

test("a claimant on the list claims their amount, once, for their wallet only", async () => {
  const c = await chain();
  const [alice, bob, carol] = await Promise.all([funded(c.svm), funded(c.svm), funded(c.svm)]);
  // The distributor's list, and the root it publishes: the circuit's own hashing.
  const claims: Claim[] = [
    { claimant: bytes(alice.address), amount: 500 },
    { claimant: bytes(bob.address), amount: 1_000 },
    { claimant: bytes(carol.address), amount: 250 },
  ];
  const { root, paths } = list(claims);
  const drop = await distribute(c, root, 1_750n);
  expect(balance(c.svm, drop.vault)).toBe(1_750n);

  // Bob proves he's on the list for 1000, at a path only he and the distributor see.
  const start = performance.now();
  const proof = prover.prove({ path: paths[1], root, claim: claims[1] }).toBytes();
  console.log(`token_claim: proved in ${Math.round(performance.now() - start)} ms`);
  const bobToken = await tokenAccount(c.svm, c.mint, bob.address, 0n);

  // Another amount, or another signer: the program fills both in, and the proof fails.
  await expect(send(c.svm, bob, await claim(drop, bob, bobToken, proof, 1_001n))).rejects.toThrow(/InvalidProof/);
  const mallory = await funded(c.svm);
  const malloryToken = await tokenAccount(c.svm, c.mint, mallory.address, 0n);
  await expect(send(c.svm, mallory, await claim(drop, mallory, malloryToken, proof, 1_000n))).rejects.toThrow(/InvalidProof/);
  // Another list, with Bob on it for the same amount: its root isn't the proof's.
  const other = list([claims[1], { claimant: bytes(mallory.address), amount: 750 }]);
  const otherDrop = await distribute(c, other.root, 1_750n);
  await expect(send(c.svm, bob, await claim(otherDrop, bob, bobToken, proof, 1_000n))).rejects.toThrow(/InvalidProof/);

  const result = await send(c.svm, bob, await claim(drop, bob, bobToken, proof, 1_000n));
  console.log(`token_claim: a claim uses ${result.computeUnitsConsumed()} compute units`);
  expect(balance(c.svm, bobToken)).toBe(1_000n);
  expect(balance(c.svm, drop.vault)).toBe(750n);

  // Bob's receipt exists, so the same claim (or a fresh proof of it) can't pay twice.
  await expect(send(c.svm, bob, await claim(drop, bob, bobToken, proof, 1_000n))).rejects.toThrow(/AccountAlreadyInitialized/);

  // Alice claims too, to an account of her choosing. Someone sent lamports to her
  // receipt's address first, to block it: the program creates it anyway.
  const [aliceReceipt] = await getProgramDerivedAddress({ programAddress: program, seeds: ["claimed", bytes(drop.distributor), bytes(alice.address)] });
  c.svm.airdrop(aliceReceipt, lamports(1_000n));
  const aliceProof = prover.prove({ path: paths[0], root, claim: claims[0] }).toBytes();
  const aliceToken = await tokenAccount(c.svm, c.mint, alice.address, 0n);
  await send(c.svm, alice, await claim(drop, alice, aliceToken, aliceProof, 500n));
  expect(balance(c.svm, aliceToken)).toBe(500n);
  expect(balance(c.svm, drop.vault)).toBe(250n); // Carol's, unclaimed
});

test("an address not on the list has no proof", async () => {
  const [alice, stranger] = await Promise.all([generateKeyPairSigner(), generateKeyPairSigner()]);
  const { root, paths } = list([{ claimant: bytes(alice.address), amount: 500 }]);
  // At Alice's path, or the next free one: no leaf of the list is the stranger's.
  for (const path of [paths[0], paths[0].next(Claim.leaf({ claimant: bytes(alice.address), amount: 500 }))]) {
    const prove = () => prover.prove({ path, root, claim: { claimant: bytes(stranger.address), amount: 500 } });
    expect(prove).toThrow(/path\.root\(claim\.leaf\(\)\)/);
  }
});
