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
import fs from 'fs';
import os from 'os';
import path from 'path';
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

/**
 * #3910 — read a REGISTERED key from disk, rather than deriving one.
 *
 * The room derived its keys from a shared secret while the graph registered each
 * identity's personal keys. Two key families for one identity: Silas rotated the
 * secret on Friday and the room silently stopped authenticating, with nothing
 * anywhere saying so. Jeff's Clearing sat empty through Sunday because of it.
 *
 * Custody (Silas, 2026-08-17): the room signs as ONE service identity — the
 * bridge — because each role already publishes its own replies through its hooks
 * daemon. The room never holds a role's private key.
 *
 * Keys live at ~/.chorus/identity/<who>/nostr.json, 0600, minted by
 * chorus-nostr-key, and the graph's credential rows carry the same pubkeys. This
 * reads; it never writes and never mints.
 */
/**
 * #3913 — ONE identity-file reader, path validated before it reaches fs
 * (security/detect-non-literal-fs-filename): `who` must be a bare slug, so a
 * traversal or absolute path can never be composed into the read. Both public
 * entry points default to this instead of an inline fs.readFileSync.
 */
function readIdentityFile(p: string): string {
  const who = path.basename(path.dirname(p));
  if (!/^[a-z0-9-]+$/.test(who) || p !== path.join(path.dirname(path.dirname(p)), who, 'nostr.json')) {
    throw new Error(`buzz-signer: refusing to read an identity path that is not <home>/.chorus/identity/<slug>/nostr.json (${p})`);
  }
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- path proven above: fixed filename under a slug-validated identity dir
  return fs.readFileSync(p, 'utf8');
}

export function registeredSigner(
  who: string,
  home: string = os.homedir(),
  read: (p: string) => string = readIdentityFile,
): NostrSigner {
  const file = path.join(home, '.chorus', 'identity', who, 'nostr.json');
  let parsed: { seckey?: unknown; pubkey?: unknown };
  try {
    parsed = JSON.parse(read(file)) as { seckey?: unknown; pubkey?: unknown };
  } catch (err) {
    // Fail loud. A room that quietly runs unsigned is the failure we are leaving
    // behind: it looks alive and delivers nothing.
    // #3913 — attach the cause instead of stringifying it (preserve-caught-error).
    throw new Error(
      `buzz-signer: cannot read the registered key for '${who}' at ${file} — `
      + 'mint it with chorus-nostr-key.',
      { cause: err },
    );
  }
  if (typeof parsed.seckey !== 'string') {
    throw new Error(`buzz-signer: ${file} has no seckey — refusing to sign with nothing`);
  }
  const signer = nobleSigner(parsed.seckey);
  // The file carries the pubkey too. If they disagree, the file is wrong about
  // itself and every signature would be attributed to a key nobody registered.
  if (typeof parsed.pubkey === 'string' && parsed.pubkey !== signer.pubkey) {
    throw new Error(
      `buzz-signer: ${file} pubkey does not match its seckey — `
      + `file says ${parsed.pubkey.slice(0, 12)}…, key derives ${signer.pubkey.slice(0, 12)}…`,
    );
  }
  return signer;
}

/** The pubkey a registered identity is known by, without touching its seckey. */
export function registeredPubkey(
  who: string,
  home: string = os.homedir(),
  read: (p: string) => string = readIdentityFile,
): string | null {
  try {
    const parsed = JSON.parse(read(path.join(home, '.chorus', 'identity', who, 'nostr.json'))) as { pubkey?: unknown };
    return typeof parsed.pubkey === 'string' ? parsed.pubkey : null;
  } catch {
    return null;   // an identity with no minted key simply cannot be attributed
  }
}
