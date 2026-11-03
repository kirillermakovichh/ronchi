import { createPrivateKey, createPublicKey, sign } from "node:crypto";
const alphabet = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
function decode(value: string) {
  let n = 0n;
  for (const c of value) n = n * 58n + BigInt(alphabet.indexOf(c));
  let hex = n.toString(16);
  if (hex.length % 2) hex = "0" + hex;
  return Buffer.concat([
    Buffer.alloc(value.match(/^1*/)?.[0].length ?? 0),
    n ? Buffer.from(hex, "hex") : Buffer.alloc(0),
  ]);
}
function encode(bytes: Buffer) {
  let n = BigInt("0x" + bytes.toString("hex")),
    value = "";
  while (n) {
    value = alphabet[Number(n % 58n)] + value;
    n /= 58n;
  }
  for (const b of bytes) {
    if (b !== 0) break;
    value = "1" + value;
  }
  return value;
}
async function rpc(address: string, method: string, params: unknown[] = []) {
  const response = await fetch(`http://${address}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }),
  });
  const value = (await response.json()) as { result: any; error?: unknown };
  if (value.error) throw new Error(`Fixture RPC ${method} failed`);
  return value.result;
}
export async function incident(address: string, seed: number) {
  const blockhash = (await rpc(address, "getLatestBlockhash")).value
    .blockhash as string;
  const key = createPrivateKey({
    key: Buffer.concat([
      Buffer.from("302e020100300506032b657004220420", "hex"),
      Buffer.alloc(32, seed),
    ]),
    format: "der",
    type: "pkcs8",
  });
  const publicKey = createPublicKey(key)
    .export({ format: "der", type: "spki" })
    .subarray(-32);
  const message = Buffer.concat([
    Buffer.from([1, 0, 0, 1]),
    publicKey,
    decode(blockhash),
    Buffer.from([0]),
  ]);
  const signature = sign(null, message, key),
    wire = Buffer.concat([Buffer.from([1]), signature, message]).toString(
      "base64",
    );
  let responseLost = false;
  try {
    await rpc(address, "sendTransaction", [
      wire,
      { encoding: "base64", skipPreflight: true },
    ]);
  } catch {
    responseLost = true;
  }
  if (!responseLost) throw new Error("Expected real response withholding");
  const identity = encode(signature);
  await rpc(address, "getSignatureStatuses", [[identity]]);
  if (
    (await rpc(address, "sendTransaction", [
      wire,
      { encoding: "base64", skipPreflight: true },
    ])) !== identity
  )
    throw new Error("Retry identity mismatch");
  return identity;
}
