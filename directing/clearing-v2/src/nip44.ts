// @domain: messages
// #4652 reopen 2 — NIP-44 v2 encryption, which NIP-46 remote signing rides on. Jeff picked
// A2 (10-20 11:53): a signing service holds a copy of his Buzz key and Clearing v2 asks it
// to sign his posts, so the room never holds his key. Spec: github.com/nostr-protocol/nips/blob/master/44.md
import { secp256k1 } from '@noble/curves/secp256k1';
import { chacha20 } from '@noble/ciphers/chacha';
import { equalBytes } from '@noble/ciphers/utils';
import { extract as hkdfExtract, expand as hkdfExpand } from '@noble/hashes/hkdf';
import { hmac } from '@noble/hashes/hmac';
import { sha256 } from '@noble/hashes/sha256';
import { concatBytes, hexToBytes, randomBytes, utf8ToBytes } from '@noble/hashes/utils';

const VERSION = 2;
const SALT = utf8ToBytes('nip44-v2');

/** The shared conversation key between a private key and an x-only public key (hex). */
export function conversationKey(privHex: string, pubHex: string): Uint8Array {
  const shared = secp256k1.getSharedSecret(hexToBytes(privHex), hexToBytes('02' + pubHex));
  return hkdfExtract(sha256, shared.subarray(1, 33), SALT);
}

function messageKeys(convKey: Uint8Array, nonce: Uint8Array) {
  const k = hkdfExpand(sha256, convKey, nonce, 76);
  return { chachaKey: k.subarray(0, 32), chachaNonce: k.subarray(32, 44), hmacKey: k.subarray(44, 76) };
}

export function paddedLength(len: number): number {
  if (len <= 32) return 32;
  const nextPower = 1 << (Math.floor(Math.log2(len - 1)) + 1);
  const chunk = nextPower <= 256 ? 32 : nextPower / 8;
  return chunk * (Math.floor((len - 1) / chunk) + 1);
}

function pad(plaintext: string): Uint8Array {
  const bytes = utf8ToBytes(plaintext);
  if (bytes.length < 1 || bytes.length > 65535) throw new Error('nip44: plaintext must be 1..65535 bytes');
  const out = new Uint8Array(2 + paddedLength(bytes.length));
  new DataView(out.buffer).setUint16(0, bytes.length);
  out.set(bytes, 2);
  return out;
}

function unpad(padded: Uint8Array): string {
  const len = new DataView(padded.buffer, padded.byteOffset).getUint16(0);
  const body = padded.subarray(2, 2 + len);
  if (len < 1 || body.length !== len || padded.length !== 2 + paddedLength(len)) throw new Error('nip44: bad padding');
  return new TextDecoder().decode(body);
}

export function encrypt(plaintext: string, convKey: Uint8Array, nonce: Uint8Array = randomBytes(32)): string {
  const { chachaKey, chachaNonce, hmacKey } = messageKeys(convKey, nonce);
  const ciphertext = chacha20(chachaKey, chachaNonce, pad(plaintext));
  const mac = hmac(sha256, hmacKey, concatBytes(nonce, ciphertext));
  return Buffer.from(concatBytes(new Uint8Array([VERSION]), nonce, ciphertext, mac)).toString('base64');
}

export function decrypt(payload: string, convKey: Uint8Array): string {
  const data = new Uint8Array(Buffer.from(payload, 'base64'));
  if (data.length < 99 || data[0] !== VERSION) throw new Error('nip44: unknown version or too short');
  const nonce = data.subarray(1, 33);
  const ciphertext = data.subarray(33, data.length - 32);
  const mac = data.subarray(data.length - 32);
  const { chachaKey, chachaNonce, hmacKey } = messageKeys(convKey, nonce);
  const expected = hmac(sha256, hmacKey, concatBytes(nonce, ciphertext));
  if (!equalBytes(expected, mac)) throw new Error('nip44: bad mac');
  return unpad(chacha20(chachaKey, chachaNonce, ciphertext));
}
