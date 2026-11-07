// @test-type: unit — NIP-44 v2 against the spec's published vector; no network.
// @domain: messages
// @card: #4652
// @owner: wren
// #4652 reopen 2 — the encryption NIP-46 remote signing rides on (Jeff picked A2, 10-20 11:53).
import { bytesToHex, hexToBytes } from '@noble/hashes/utils';
import { conversationKey, decrypt, encrypt, paddedLength } from '../src/nip44';
import { schnorr } from '@noble/curves/secp256k1';

const SEC1 = '0000000000000000000000000000000000000000000000000000000000000001';
const SEC2 = '0000000000000000000000000000000000000000000000000000000000000002';
const pub = (sec: string) => bytesToHex(schnorr.getPublicKey(hexToBytes(sec)));

describe('#4652 NIP-44 v2', () => {
  test('the spec vector: conversation key and payload for "a"', () => {
    const ck = conversationKey(SEC1, pub(SEC2));
    expect(bytesToHex(ck)).toBe('c41c775356fd92eadc63ff5a0dc1da211b268cbea22316767095b2871ea1412d');
    const nonce = hexToBytes('0000000000000000000000000000000000000000000000000000000000000001');
    expect(encrypt('a', ck, nonce)).toBe('AgAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABee0G5VSK0/9YypIObAtDKfYEAjD35uVkHyB0F4DwrcNaCXlCWZKaArsGrY6M9wnuTMxWfp1RTN9Xga8no+kF5Vsb');
  });

  test('both sides derive the same key, and a message round-trips', () => {
    const a = conversationKey(SEC1, pub(SEC2));
    const b = conversationKey(SEC2, pub(SEC1));
    expect(bytesToHex(a)).toBe(bytesToHex(b));
    expect(decrypt(encrypt('{"id":"1","method":"sign_event"}', a), b)).toBe('{"id":"1","method":"sign_event"}');
  });

  test('NEGATIVE PROOF: a tampered payload is refused, not decrypted', () => {
    const ck = conversationKey(SEC1, pub(SEC2));
    const p = Buffer.from(encrypt('hello', ck), 'base64');
    p[40] ^= 1;
    expect(() => decrypt(p.toString('base64'), ck)).toThrow('bad mac');
  });

  test('padding follows the spec table', () => {
    expect([1, 32, 33, 64, 65, 256, 257, 1000].map(paddedLength)).toEqual([32, 32, 64, 64, 96, 256, 320, 1024]);
  });
});
