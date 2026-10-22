// A shielded SPL token from the wallets' side: Alice deposits 150, pays Bob 100
// privately (50 back to her as change), and Bob withdraws his 100. The note
// algebra (`Key`, `Note`, `Path`) is the circuit's own Rust, run from TypeScript;
// the wallet only tracks which notes and paths are whose.
import {
  AccountRole,
  address,
  generateKeyPairSigner,
  getAddressEncoder,
  getProgramDerivedAddress,
  lamports,
  type Address,
  type Instruction,
  type KeyPairSigner,
} from "@solana/kit";
import { getSetComputeUnitLimitInstruction } from "@solana-program/compute-budget";
import { AccountState, TOKEN_PROGRAM_ADDRESS, getMintEncoder, getMintSize, getTokenDecoder, getTokenEncoder, getTokenSize } from "@solana-program/token";
import type { LiteSVM } from "litesvm";
import { createHash } from "node:crypto";
import { anchorData, artifacts, funded, pooled, send, u64le, vm } from "./harness";
import { Field, Key, Path, ShieldedTransact, Transaction, type Note, type PathValue, type ShieldedTransactProver, type Spend } from "./shielded_transact";

const program = address("CUCcJJBRbK6tK4nPP2zgmvdYbKWSCGGVdRegtJ2GPJtF");
const system = address("11111111111111111111111111111111");
const rent = address("SysvarRent111111111111111111111111111111111");
const bytes = (a: Address) => new Uint8Array(getAddressEncoder().encode(a));
const random = () => new Field(crypto.getRandomValues(new Uint8Array(31)));
const signer = (s: KeyPairSigner) => ({ address: s.address, role: AccountRole.WRITABLE_SIGNER, signer: s });
const writable = (a: Address) => ({ address: a, role: AccountRole.WRITABLE });
const readonly = (a: Address) => ({ address: a, role: AccountRole.READONLY });
/** A Borsh `Vec<u8>`: its length as a `u32`, then its bytes. */
const vec = (b: Uint8Array) => new Uint8Array([...new Uint8Array(new Uint32Array([b.length]).buffer), ...b]);

/** A memo's SHA-256, as the circuit's `Hash` takes it and the program computes it. */
const sha256 = (memo: Uint8Array) => new Uint8Array(createHash("sha256").update(memo).digest());

let prover: ShieldedTransactProver;
let threads: Awaited<ReturnType<typeof pooled>>;
beforeAll(async () => {
  threads = await pooled();
  prover = await ShieldedTransact.prover(artifacts("circuits/transact", "shielded_transact"), { wasm: threads.wasm, digest: ShieldedTransact.digest });
});
afterAll(() => threads.terminate());

/** A note a wallet holds: enough to spend it. */
interface Owned {
  note: Note;
  diversifier: Field;
  path: PathValue;
}

/** A pool for a fresh mint, and what every wallet knows of its tree. */
async function pool() {
  const svm = vm(program, "program/target/deploy/shielded_transfer.so");
  const payer = await funded(svm);
  const mint = (await generateKeyPairSigner()).address;
  const mintData = getMintEncoder().encode({ mintAuthority: payer.address, supply: 1_000n, decimals: 0, isInitialized: true, freezeAuthority: null });
  svm.setAccount({ address: mint, data: new Uint8Array(mintData), executable: false, lamports: lamports(svm.minimumBalanceForRentExemption(BigInt(getMintSize()))), programAddress: TOKEN_PROGRAM_ADDRESS, space: BigInt(getMintSize()) });
  const [poolAddress] = await getProgramDerivedAddress({ programAddress: program, seeds: ["pool", bytes(mint)] });
  const [vault] = await getProgramDerivedAddress({ programAddress: program, seeds: ["vault", bytes(mint)] });
  await send(svm, payer, {
    programAddress: program,
    accounts: [writable(poolAddress), writable(vault), readonly(mint), signer(payer), readonly(TOKEN_PROGRAM_ADDRESS), readonly(system), readonly(rent)],
    data: anchorData("initialize"),
  });
  // The program's empty root is the circuit's: the first path's tree root.
  const account = svm.getAccount(poolAddress);
  const tip = Path.first();
  expect(account.exists && account.data.subarray(48, 80)).toEqual(tip.treeRoot().toBytes());
  return { svm, mint, poolAddress, vault, tip, held: [] as Owned[] };
}
type Pool = Awaited<ReturnType<typeof pool>>;

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

/** Spend nothing: a dummy, worth 0, at any path. */
const dummy = (key: Key): Spend => {
  const diversifier = random();
  return { note: { value: 0, owner: Key.address(key, diversifier), rho: random(), rseed: random() }, diversifier, path: Path.first(), real: false };
};
const spend = ({ note, diversifier, path }: Owned): Spend => ({ note, diversifier, path, real: true });

/** A new note, and the owner's diversifier it's sent to. */
type Output = { note: Note; diversifier: Field };

/** A new note of `value` for `owner`, and the diversifier it's sent to. */
const pay = (owner: Key, value: bigint) => {
  const diversifier = random();
  return { diversifier, note: { value, owner: Key.address(owner, diversifier), rho: random(), rseed: random() } };
};

/**
 * A transact can need more than the default 200k compute units: measured at 193k to
 * 204k, varying with its public inputs (the verifier skips the scalar multiplication
 * for a small one). Ask for 250k.
 */
const budget = getSetComputeUnitLimitInstruction({ units: 250_000 });

/** What a transaction moves, and the token accounts it moves it between. */
interface Moves {
  vpubIn?: bigint; // from `userToken`
  vpubOut?: bigint; // to `recipient`
  fee?: bigint; // to `relayerToken`, the submitter's
  userToken?: Address;
  recipient?: Address;
  relayerToken?: Address;
}

/**
 * One transaction, proved: `key` spends `spends` and creates `outputs` (each with its
 * owner's diversifier). `instruction()` is what to send; `landed()` records it in
 * every wallet once it has. An account a transaction doesn't move tokens with is
 * left out: its slot holds the program's address (Anchor's "none"), and the proof
 * binds the program's address in its place.
 */
async function prepare(p: Pool, key: Key, spends: [Spend, Spend], outputs: [Output, Output], moves: Moves = {}) {
  const { vpubIn = 0n, vpubOut = 0n, fee = 0n } = moves;
  const tip = p.tip;
  const memos = outputs.map(() => crypto.getRandomValues(new Uint8Array(48))); // each note, encrypted to its owner
  // The statement the program checks, from the circuit's own Rust.
  const flow = { vpubIn, vpubOut, fee, recipient: bytes(moves.recipient ?? program), relayer: bytes(moves.relayerToken ?? program) };
  const notes = [outputs[0].note, outputs[1].note] as const;
  const tx = Transaction.of(key, spends, notes, tip, bytes(p.mint), flow, [sha256(memos[0]), sha256(memos[1])]);
  const proof = prover.prove({ key, spends, outputs: notes, tip, tx });
  const [n0, n1] = await Promise.all(tx.nullifiers.map((n) => getProgramDerivedAddress({ programAddress: program, seeds: ["nullifier", bytes(p.mint), n.toBytes()] })));
  const optional = (a?: Address) => (a ? writable(a) : readonly(program));

  return {
    /** Sent by `user`, naming the accounts in `moves` (or `accounts`, to try others). */
    instruction: (user: KeyPairSigner, accounts: Moves = moves): Instruction => ({
      programAddress: program,
      accounts: [
        writable(p.poolAddress), writable(p.vault), writable(n0[0]), writable(n1[0]), signer(user),
        optional(accounts.userToken), optional(accounts.recipient), optional(accounts.relayerToken),
        readonly(TOKEN_PROGRAM_ADDRESS), readonly(system),
      ],
      data: anchorData(
        "transact", proof.toBytes(), tx.root.toBytes(), ...tx.nullifiers.map((n) => n.toBytes()), ...tx.commitments.map((c) => c.toBytes()),
        tx.newRoot.toBytes(), u64le(vpubIn), u64le(vpubOut), u64le(fee), vec(memos[0]), vec(memos[1]),
      ),
    }),
    /** Every wallet watching the pool appends the two new leaves to its paths. */
    landed(): Owned[] {
      const created: Owned[] = [];
      for (const [i, leaf] of tx.commitments.entries()) {
        const at = p.tip;
        for (const held of [...p.held, ...created]) held.path = held.path.update(at, leaf);
        created.push({ ...outputs[i], path: at });
        p.tip = at.next(leaf);
      }
      p.held.push(...created);
      return created;
    },
  };
}

/** Prove and send one transaction, signed by `user`; returns the new notes. */
async function transact(p: Pool, user: KeyPairSigner, key: Key, spends: [Spend, Spend], outputs: [Output, Output], moves: Moves = {}) {
  const t = await prepare(p, key, spends, outputs, moves);
  await send(p.svm, user, budget, t.instruction(user));
  return t.landed();
}

test("deposit, pay privately, withdraw, through a relayer", async () => {
  const p = await pool();
  const [alice, bob] = [{ secret: random() }, { secret: random() }];
  const aliceSol = await funded(p.svm);
  const aliceToken = await tokenAccount(p.svm, p.mint, aliceSol.address, 150n);
  // Bob holds no SOL at all: a relayer submits for him, for a fee in the token.
  const bobToken = await tokenAccount(p.svm, p.mint, (await generateKeyPairSigner()).address, 0n);
  const relayer = await funded(p.svm);
  const relayerToken = await tokenAccount(p.svm, p.mint, relayer.address, 0n);

  // 1. Alice shields 150: nothing spent, 150 in.
  const [deposited] = await transact(p, aliceSol, alice, [dummy(alice), dummy(alice)], [pay(alice, 150n), pay(alice, 0n)], { vpubIn: 150n, userToken: aliceToken });
  expect(balance(p.svm, p.vault)).toBe(150n);
  expect(balance(p.svm, aliceToken)).toBe(0n);

  // 2. She pays Bob 100 and keeps 45, through the relayer for 5: no wallet of hers
  //    signs, and nothing public says who paid whom.
  const [toBob] = await transact(p, relayer, alice, [spend(deposited), dummy(alice)], [pay(bob, 100n), pay(alice, 45n)], { fee: 5n, relayerToken });
  expect(balance(p.svm, relayerToken)).toBe(5n);
  expect(balance(p.svm, p.vault)).toBe(145n);

  // 3. Bob unshields 95 to his token account, 5 to the relayer. The proof names both
  //    accounts: whoever submits it can't redirect either.
  const elsewhere = await tokenAccount(p.svm, p.mint, relayer.address, 0n);
  const withdrawal = await prepare(p, bob, [spend(toBob), dummy(bob)], [pay(bob, 0n), pay(bob, 0n)], { vpubOut: 95n, fee: 5n, recipient: bobToken, relayerToken });
  const thief = await funded(p.svm);
  await expect(send(p.svm, thief, budget, withdrawal.instruction(thief, { recipient: bobToken, relayerToken: elsewhere }))).rejects.toThrow(/InvalidProof/);
  await expect(send(p.svm, thief, budget, withdrawal.instruction(thief, { recipient: elsewhere, relayerToken }))).rejects.toThrow(/InvalidProof/);
  await send(p.svm, relayer, budget, withdrawal.instruction(relayer));
  withdrawal.landed();
  expect(balance(p.svm, bobToken)).toBe(95n);
  expect(balance(p.svm, relayerToken)).toBe(10n);
  expect(balance(p.svm, p.vault)).toBe(45n); // Alice's change, still shielded

  // Bob's note is spent: its nullifier exists, so it can't come out twice.
  await expect(send(p.svm, relayer, budget, withdrawal.instruction(relayer))).rejects.toThrow(/already in use/);
});

test("only a note's owner can spend it", async () => {
  const p = await pool();
  const [alice, mallory] = [{ secret: random() }, { secret: random() }];
  const user = await funded(p.svm);
  const token = await tokenAccount(p.svm, p.mint, user.address, 150n);
  const [note] = await transact(p, user, alice, [dummy(alice), dummy(alice)], [pay(alice, 150n), pay(alice, 0n)], { vpubIn: 150n, userToken: token });
  await expect(
    transact(p, user, mallory, [spend(note), dummy(mallory)], [pay(mallory, 150n), pay(mallory, 0n)]),
  ).rejects.toThrow(/key\.address\(spend\.diversifier\)/);
});
