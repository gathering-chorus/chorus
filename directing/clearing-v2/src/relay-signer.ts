// @domain: messages
// #4571 — the room's reader key: the registered bridge identity v1's Clearing reads
// #team with (~/.chorus/identity/bridge/nostr.json, minted by chorus-nostr-key).
// The key stays in that file; this hands the relay client a signer, never the key.
import fs from 'fs';
import path from 'path';
import { schnorr } from '@noble/curves/secp256k1';
import { bytesToHex, hexToBytes } from '@noble/hashes/utils';
import type { RelaySigner } from './relay-client';

export function bridgeSigner(home: string): { signer?: RelaySigner; reason?: string } {
  const file = path.join(home, '.chorus', 'identity', 'bridge', 'nostr.json');
  let parsed: { seckey?: unknown; pubkey?: unknown };
  try {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- fixed filename under the caller's home
    parsed = JSON.parse(fs.readFileSync(file, 'utf8')) as { seckey?: unknown; pubkey?: unknown };
  } catch {
    return { reason: `no reader key at ${file}` };
  }
  if (typeof parsed.seckey !== 'string' || !/^[0-9a-f]{64}$/i.test(parsed.seckey)) {
    return { reason: `${file} has no 32-byte hex seckey` };
  }
  const sk = hexToBytes(parsed.seckey);
  const pubkey = bytesToHex(schnorr.getPublicKey(sk));
  if (typeof parsed.pubkey === 'string' && parsed.pubkey !== pubkey) {
    return { reason: `${file} pubkey does not match its seckey` };
  }
  return { signer: { pubkey, sign: (idHex: string) => bytesToHex(schnorr.sign(hexToBytes(idHex), sk)) } };
}
