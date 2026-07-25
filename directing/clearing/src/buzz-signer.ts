/**
 * #3674 — the live @noble-backed NostrSigner for the mirror bridge. This is the
 * ONE place secp256k1 Schnorr + sha256 are imported (pure ESM, @noble 2.x). Kept
 * out of the unit-test path on purpose — the bridge's logic is tested with a stub
 * signer; THIS adapter is proven live, where a bad signature is rejected by the
 * relay at the door (the honest proof for crypto).
 *
 * Key material: read from ~/.chorus/buzz/bridge.key (0600) into env var
 * BUZZ_BRIDGE_NOSTR_KEY by the bridge's startup — the #3618 keyId convention
 * (name in the graph, value only in the file/env, never in the graph).
 */
import { schnorr } from '@noble/curves/secp256k1';
import { sha256 } from '@noble/hashes/sha256';
import { bytesToHex, hexToBytes, utf8ToBytes } from '@noble/hashes/utils';
import type { NostrSigner } from './buzz-bridge';

/** Build a live signer from the bridge's 32-byte hex private key. */
export function nobleSigner(privKeyHex: string): NostrSigner {
  if (!privKeyHex || !/^[0-9a-f]{64}$/i.test(privKeyHex)) {
    throw new Error('buzz-signer: BUZZ_BRIDGE_NOSTR_KEY must be 32-byte hex');
  }
  const sk = hexToBytes(privKeyHex);
  const pubkey = bytesToHex(schnorr.getPublicKey(sk));
  return {
    pubkey,
    signEvent(serialized: string) {
      const id = bytesToHex(sha256(utf8ToBytes(serialized)));
      const sig = bytesToHex(schnorr.sign(hexToBytes(id), sk));
      return { id, sig };
    },
  };
}
