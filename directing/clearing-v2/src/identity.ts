// @domain: messages

import { schnorr } from '@noble/curves/secp256k1';
import { sha256 } from '@noble/hashes/sha256';
import { bytesToHex, hexToBytes, utf8ToBytes } from '@noble/hashes/utils';
import { NostrEvent } from './types';

/**
 * Standard NIP-01 canonical event serialization:
 * [0, <pubkey>, <created_at>, <kind>, <tags>, <content>]
 */
export function serializeEvent(
  pubkey: string,
  createdAt: number,
  kind: number,
  tags: string[][],
  content: string,
): string {
  return JSON.stringify([0, pubkey.toLowerCase(), createdAt, kind, tags, content]);
}

/**
 * Build 32-byte hex ID from canonical event serialization.
 */
export function buildEventId(serialized: string): string {
  return bytesToHex(sha256(utf8ToBytes(serialized)));
}

/**
 * Verify a Nostr event's ID and Schnorr signature against its declared pubkey.
 */
export function verifyNostrEvent(event: NostrEvent): boolean {
  try {
    if (!event.id || !event.pubkey || !event.sig) return false;
    if (event.id.length !== 64 || event.pubkey.length !== 64 || event.sig.length !== 128) {
      return false;
    }

    // 1. Check ID calculation
    const serialized = serializeEvent(
      event.pubkey,
      event.created_at,
      event.kind,
      event.tags,
      event.content,
    );
    const expectedId = buildEventId(serialized);
    if (event.id.toLowerCase() !== expectedId.toLowerCase()) {
      return false;
    }

    // 2. Verify Schnorr signature
    const idBytes = hexToBytes(event.id);
    const sigBytes = hexToBytes(event.sig);
    const pubkeyBytes = hexToBytes(event.pubkey);

    return schnorr.verify(sigBytes, idBytes, pubkeyBytes);
  } catch {
    return false;
  }
}

/**
 * Map a verified Nostr hex pubkey to a Chorus role.
 */
export function resolveRoleByPubkey(
  pubkey: string,
  knownKeys: Map<string, string>,
): string | null {
  if (!pubkey) return null;
  return knownKeys.get(pubkey.toLowerCase()) || null;
}

/**
 * #3743 — who is speaking, from the cryptographic signature that already happened.
 *
 * In v1 (#3743), the old resolver read a client-supplied field or defaulted to 'jeff'.
 * An authenticated Mark rendered as Jeff and had Jeff's authority.
 * In v2, the sender comes strictly from the verified cryptographic event (Schnorr signature),
 * never from a client-supplied body field and never from a 'jeff' default.
 */
export interface SenderIdentity {
  name: string;
  role: string;
  pubkey: string;
  authenticated: boolean;
}

export function resolveSenderIdentity(
  event: NostrEvent | undefined,
  knownKeys: Map<string, string>,
): SenderIdentity | null {
  if (!event || !verifyNostrEvent(event)) {
    return null;
  }
  const role = resolveRoleByPubkey(event.pubkey, knownKeys);
  if (!role) {
    return null;
  }
  return {
    name: role,
    role,
    pubkey: event.pubkey,
    authenticated: true,
  };
}

