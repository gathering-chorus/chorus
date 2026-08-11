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

/**
 * #3823 — per-actor signing. The spike signed everything with one bridge key
 * and kept the true author in the content as "[wren] …", which is a label: the
 * room could not distinguish a real Wren message from anything else holding the
 * bridge key. Each actor now signs as itself, so a message's identity IS its
 * signature.
 *
 * The key is DERIVED from the actor's WebID, which is the property the whole
 * Buzz question turns on. Derived-and-bound means role config and authorization
 * travel unchanged and Buzz is a transport swap; free-standing would mean two
 * identity systems and every policy written twice.
 *
 * The secret is NOT in this file. Silas and I agreed a shared constant for the
 * test-drive and I wrote it here; the pre-commit scanner refused it, correctly.
 * A secret that derives four private keys, committed to a repo three roles and
 * a tunnel can read, is key material in source — anyone holding the checkout
 * could sign as Jeff. The scanner caught what the convenience of a one-day
 * spike had talked me out of noticing.
 *
 * So it comes from the environment (BUZZ_ROOM_SECRET), which is the #3618
 * convention already in use here: the name lives in code and the graph, the
 * value only ever in a file or env. Callers that want a specific secret — the
 * tests — pass one explicitly.
 */
export function roomSecret(env: NodeJS.ProcessEnv = process.env): string {
  const secret = env.BUZZ_ROOM_SECRET;
  if (!secret) {
    throw new Error(
      'buzz-signer: BUZZ_ROOM_SECRET is not set — refusing to derive keys from a default. '
      + 'A fallback here would silently give every actor a second identity that anyone '
      + 'reading the source could forge.',
    );
  }
  return secret;
}

/** An actor's WebID, in the domains:security shape. */
export function webIdFor(actor: string): string {
  return `https://id.lightlifeurbangardens.com/${actor}/profile/card#me`;
}

/**
 * Derive an actor's signing key from its WebID:
 *   sk = sha256(utf8(secret + "|" + webId))   — raw 32 bytes, never hex-then-hash
 *
 * One byte of divergence here gives each actor two identities and makes the
 * test meaningless, so this function and Silas's must stay identical. Verified
 * both directions on 2026-08-11 (jeff f84cd052…, wren 31557ee6…, silas
 * a220691b…, kade cc2582e0…).
 */
export function derivedSigner(actor: string, secret: string = roomSecret()): NostrSigner {
  const sk = sha256(utf8ToBytes(`${secret}|${webIdFor(actor)}`));
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
