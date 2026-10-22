// The pool from a wallet's side: deposit notes, keep a note's Merkle path current
// as others deposit, and withdraw through a relayer, all with the generated clients.
// The tree logic (`Path`, `Note`) is the circuits' own Rust, run from TypeScript.
import {
  AccountRole,
  address,
  getAddressEncoder,
  getProgramDerivedAddress,
  type Address,
  type Instruction,
  type KeyPairSigner,
} from "@solana/kit";
import type { LiteSVM } from "litesvm";
import { anchorData, artifacts, funded, pooled, send, u64le, vm } from "./harness";
import { Append, Path, ShieldedPoolDeposit, type PathValue, type ShieldedPoolDepositProver } from "./shielded_pool_deposit";
import { Field, Note, ShieldedPoolWithdraw, Withdrawal, type ShieldedPoolWithdrawProver } from "./shielded_pool_withdraw";

const program = address("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS");
const system = address("11111111111111111111111111111111");
const DENOMINATION = 1_000_000_000n; // 1 SOL
const bytes = (a: Address) => new Uint8Array(getAddressEncoder().encode(a));
const signer = (s: KeyPairSigner) => ({ address: s.address, role: AccountRole.WRITABLE_SIGNER, signer: s });

let deposits: ShieldedPoolDepositProver;
let withdrawals: ShieldedPoolWithdrawProver;
let threads: Awaited<ReturnType<typeof pooled>>;
beforeAll(async () => {
  threads = await pooled();
  // Pinned to the circuits these clients were generated from: a deployment can't swap them.
  const { wasm } = threads;
  deposits = await ShieldedPoolDeposit.prover(artifacts("circuits/deposit", "shielded_pool_deposit"), { wasm, digest: ShieldedPoolDeposit.digest });
  withdrawals = await ShieldedPoolWithdraw.prover(artifacts("circuits/withdraw", "shielded_pool_withdraw"), { wasm, digest: ShieldedPoolWithdraw.digest });
});
afterAll(() => threads.terminate());

/** A fresh note: two random field elements, kept by the depositor. */
const newNote = (): Note => ({
  nullifier: new Field(crypto.getRandomValues(new Uint8Array(31))),
  secret: new Field(crypto.getRandomValues(new Uint8Array(31))),
});

/** A pool of `DENOMINATION` deposits, and the wallet's view of its tree. */
async function pool() {
  const svm = vm(program, "program/target/deploy/shielded_pool.so");
  const [poolAddress] = await getProgramDerivedAddress({ programAddress: program, seeds: ["pool", u64le(DENOMINATION)] });
  const authority = await funded(svm);
  await send(svm, authority, {
    programAddress: program,
    accounts: [{ address: poolAddress, role: AccountRole.WRITABLE }, signer(authority), { address: system, role: AccountRole.READONLY }],
    data: anchorData("initialize", u64le(DENOMINATION)),
  });
  // The program's empty root is the circuit's: the first path's tree root.
  const account = svm.getAccount(poolAddress);
  expect(account.exists && account.data.subarray(24, 56)).toEqual(Path.first().treeRoot().toBytes());
  return { svm, poolAddress, tip: Path.first() };
}

/** Deposit `note`: prove the program's tree with its commitment appended. */
async function deposit(p: { svm: LiteSVM; poolAddress: Address; tip: PathValue }, note: Note) {
  const leaf = Note.commitment(note);
  const path = p.tip; // where the note lands
  const append = Append.of(path, leaf); // the statement the program checks
  const proof = deposits.prove({ path, append });
  const depositor = await funded(p.svm);
  await send(p.svm, depositor, {
    programAddress: program,
    accounts: [{ address: p.poolAddress, role: AccountRole.WRITABLE }, signer(depositor), { address: system, role: AccountRole.READONLY }],
    data: anchorData("deposit", leaf.toBytes(), append.newRoot.toBytes(), proof.toBytes()),
  });
  p.tip = path.next(leaf);
  return { path, leaf };
}

/** A withdrawal instruction, sent by `relayer`. */
async function withdrawal(poolAddress: Address, relayer: KeyPairSigner, recipient: Address, proof: Uint8Array, root: Field, nullifierHash: Field, fee: bigint): Promise<Instruction> {
  const [nullifier] = await getProgramDerivedAddress({ programAddress: program, seeds: ["nullifier", u64le(DENOMINATION), nullifierHash.toBytes()] });
  return {
    programAddress: program,
    accounts: [
      { address: poolAddress, role: AccountRole.WRITABLE },
      { address: nullifier, role: AccountRole.WRITABLE },
      { address: recipient, role: AccountRole.WRITABLE },
      signer(relayer),
      { address: system, role: AccountRole.READONLY },
    ],
    data: anchorData("withdraw", proof, root.toBytes(), nullifierHash.toBytes(), u64le(fee)),
  };
}

test("deposit, others deposit, withdraw unlinkably through a relayer, once", async () => {
  const p = await pool();
  const mine = newNote();
  let { path } = await deposit(p, mine);
  // Someone deposits after me: my path changes, and the wallet updates it.
  const later = p.tip;
  const { leaf } = await deposit(p, newNote());
  path = path.update(later, leaf);

  // A relayer submits for a fresh recipient, for a fee, so the recipient needs no SOL.
  const relayer = await funded(p.svm);
  const recipient = (await funded(p.svm, 0n)).address;
  const fee = 10_000_000n;
  const statement = Withdrawal.of(mine, path, bytes(recipient), bytes(relayer.address), fee);
  expect(statement.root).toEqual(p.tip.treeRoot()); // the root the program has now
  const proof = withdrawals.prove({ note: mine, path, withdrawal: statement });
  const { root, nullifierHash } = statement;
  const ix = (to: Address, f = fee, r = root) => withdrawal(p.poolAddress, relayer, to, proof.toBytes(), r, nullifierHash, f);

  // The proof pays only the recipient and fee it was made for, against a known root.
  const other = (await funded(p.svm, 0n)).address;
  await expect(send(p.svm, relayer, await ix(other))).rejects.toThrow(/InvalidProof/);
  await expect(send(p.svm, relayer, await ix(recipient, fee + 1n))).rejects.toThrow(/InvalidProof/);
  await expect(send(p.svm, relayer, await ix(recipient, fee, new Field(9)))).rejects.toThrow(/UnknownRoot/);

  const before = p.svm.getBalance(relayer.address)!;
  await send(p.svm, relayer, await ix(recipient));
  expect(p.svm.getBalance(recipient)).toBe(DENOMINATION - fee);
  // The relayer pays the transaction fee and the nullifier account's rent out of `fee`.
  expect(p.svm.getBalance(relayer.address)! > before).toBe(true);

  // The nullifier is spent: the same note can't come out twice.
  await expect(send(p.svm, relayer, await ix(recipient))).rejects.toThrow(/already in use/);
});

test("a note that isn't in the tree has no proof", async () => {
  const p = await pool();
  await deposit(p, newNote());
  const stranger = newNote();
  const zero = new Uint8Array(32);
  // Against the pool's root: a statement of the stranger's own would name a root no pool has.
  const withdrawal = { ...Withdrawal.of(stranger, Path.first(), zero, zero, 0), root: p.tip.treeRoot() };
  const prove = () => withdrawals.prove({ note: stranger, path: Path.first(), withdrawal });
  expect(prove).toThrow(/path\.root\(note\.commitment\(\)\)/);
});
