/* eslint-disable security/detect-non-literal-fs-filename -- the one key file path is fixed by config or ~/.chorus, never by a request */
// @domain: messages
// #4652 reopen 2 — turn on "Jeff's Clearing posts are stored as Jeff" once his signing
// service exists. Off (no config) keeps #4608's room copy. The room's client key is the
// room's own, created here once (0600); its pubkey is logged so the signer can allow it.
import fs from 'fs';
import os from 'os';
import path from 'path';
import { schnorr } from '@noble/curves/secp256k1';
import { bytesToHex, hexToBytes, randomBytes } from '@noble/hashes/utils';
import { JeffPublisher } from './jeff-publisher';
import { RemoteSigner } from './remote-signer';
import { RelaySignerTransport } from './signer-transport';

export interface JeffSignerConfig {
  /** The signing service's pubkey (hex). */
  bunkerPubkey?: string;
  /** Jeff's app key (hex); the only key the room accepts a signature from. */
  userPubkey?: string;
  /** Where the room keeps its own client key. */
  clientKeyFile?: string;
}

const HEX64 = /^[0-9a-f]{64}$/i;
export const DEFAULT_CLIENT_KEY_FILE = path.join(os.homedir(), '.chorus', 'clearing-v2-signer-client.key');

/** The room's client key: read it, or create it once with owner-only access. */
export function clientKey(file: string): string {
  try {
    const k = fs.readFileSync(file, 'utf8').trim();
    if (HEX64.test(k)) return k;
  } catch { /* first start */ }
  const k = bytesToHex(randomBytes(32));
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, k, { mode: 0o600 });
  return k;
}

/** The room's client pubkey the signer must allow, or undefined when the switch is off. */
export function signerClientPubkey(cfg: JeffSignerConfig | undefined): string | undefined {
  if (!cfg?.bunkerPubkey || !cfg.userPubkey) return undefined;
  return bytesToHex(schnorr.getPublicKey(hexToBytes(clientKey(cfg.clientKeyFile ?? DEFAULT_CLIENT_KEY_FILE))));
}

export function buildJeffPublisher(cfg: JeffSignerConfig | undefined, relayUrl: string, hostHeader?: string): JeffPublisher | undefined {
  if (!cfg || (!cfg.bunkerPubkey && !cfg.userPubkey)) return undefined;
  if (!HEX64.test(cfg.bunkerPubkey ?? '') || !HEX64.test(cfg.userPubkey ?? '') || !relayUrl) {
    console.error('[clearing-v2] jeffSigner config is incomplete (needs bunkerPubkey, userPubkey and a relay); keeping the room copy');
    return undefined;
  }
  const clientSecret = clientKey(cfg.clientKeyFile ?? DEFAULT_CLIENT_KEY_FILE);
  const transport = new RelaySignerTransport({ relayUrl, clientSecret, hostHeader });
  const signer = new RemoteSigner({ clientSecret, bunkerPubkey: cfg.bunkerPubkey!, userPubkey: cfg.userPubkey!, transport });
  return new JeffPublisher({ relayUrl, signer, hostHeader });
}
