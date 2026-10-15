// @test-type: unit — verifies Nostr Schnorr signature verification and role attribution, no live services.
// @card: #4530
// @owner: abby-normal
// @domain: messages

import { schnorr } from '@noble/curves/secp256k1';
import { sha256 } from '@noble/hashes/sha256';
import { bytesToHex, hexToBytes, utf8ToBytes } from '@noble/hashes/utils';
import { verifyNostrEvent, resolveRoleByPubkey, buildEventId, serializeEvent } from '../src/identity';
import { NostrEvent } from '../src/types';

describe('Clearing v2: Cryptographic Identity & Attribution', () => {
  // Deterministic test keypair (synthetic byte array, no static secret string)
  const testPrivKeyBytes = new Uint8Array(32).fill(42);
  const testPubKeyHex = bytesToHex(schnorr.getPublicKey(testPrivKeyBytes));

  const knownKeys = new Map<string, string>([
    [testPubKeyHex, 'wren'],
    ['b9d4bfd85efd8aeeacaef985d4e12325cb1d7d527755be875a3813836e06d9b7', 'abby-normal'],
  ]);

  function signTestEvent(content: string, createdAt = 1786464000): NostrEvent {
    const serialized = serializeEvent(testPubKeyHex, createdAt, 1, [['t', 'team']], content);
    const id = buildEventId(serialized);
    const sig = bytesToHex(schnorr.sign(hexToBytes(id), testPrivKeyBytes));
    return {
      id,
      pubkey: testPubKeyHex,
      created_at: createdAt,
      kind: 1,
      tags: [['t', 'team']],
      content,
      sig,
    };
  }

  test('valid kind:1 event signature is verified and attributed to role', () => {
    const event = signTestEvent('Hello from Wren');
    const isSigValid = verifyNostrEvent(event);
    expect(isSigValid).toBe(true);

    const role = resolveRoleByPubkey(event.pubkey, knownKeys);
    expect(role).toBe('wren');
  });

  test('NEGATIVE PROOF: tampered content fails cryptographic verification', () => {
    const event = signTestEvent('Original content');
    // Tamper with the content after signing
    const tampered = { ...event, content: 'Tampered content' };
    const isValid = verifyNostrEvent(tampered);
    expect(isValid).toBe(false);
  });

  test('NEGATIVE PROOF: forged signature fails verification', () => {
    const event = signTestEvent('Authentic content');
    // Replace with a garbage signature
    const forged = { ...event, sig: '0'.repeat(128) };
    const isValid = verifyNostrEvent(forged);
    expect(isValid).toBe(false);
  });

  test('NEGATIVE PROOF: unknown pubkey is unattributed', () => {
    const unknownPubkey = 'f'.repeat(64);
    const role = resolveRoleByPubkey(unknownPubkey, knownKeys);
    expect(role).toBeNull();
  });
});
