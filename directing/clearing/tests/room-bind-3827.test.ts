// @test-type: unit — calls the pure bind decision directly; no server, no relay, no pod.
/**
 * #3827 — may this browser's key speak as this person?
 *
 * The claim made to Jeff is that a message in the room is signed by a key only
 * he can use. These tests make that claim falsifiable. The failure they aim at
 * is not a crash — it is a bind that SUCCEEDS when it should not, because a key
 * bound to nobody produces messages that render as nobody, and this morning we
 * lost an afternoon to exactly that shape: a refusal naming the wrong state.
 */

import { decideBind } from '../src/room-bind';

const WEBID = 'https://id.lightlifeurbangardens.com/jeff/profile/card#me';
const PUBKEY = 'a1b2c3d4'.repeat(8); // 64 hex chars

describe('#3827 bind requires a signed-in identity', () => {
  test('a signed-in WebID with a well-formed key binds', () => {
    const out = decideBind(WEBID, PUBKEY);
    expect(out.ok).toBe(true);
    if (out.ok) {
      expect(out.webid).toBe(WEBID);
      expect(out.pubkey).toBe(PUBKEY.toLowerCase());
    }
  });

  test('an unauthenticated caller is refused, and the refusal names the state', () => {
    const out = decideBind(null, PUBKEY);
    expect(out.ok).toBe(false);
    if (!out.ok) {
      // "not signed in" is a different situation from "bad key". A refusal that
      // says only "refused" sends the reader to the wrong half of the system —
      // which is precisely how the relay's "must include an h tag" cost an hour
      // this morning when it meant "you are not a member".
      expect(out.error).toBe('not-signed-in');
      expect(out.status).toBe(401);
      expect(out.detail).toMatch(/sign in/i);
    }
  });

  test('NEGATIVE PROOF: identity is checked BEFORE the key', () => {
    // If the key were validated first, an unauthenticated caller sending
    // garbage would be told "bad-pubkey" — which tells them their session was
    // accepted and only the key was wrong. Both wrong at once must still fail
    // on identity.
    const out = decideBind(null, 'not-a-key');
    expect(out.ok).toBe(false);
    if (!out.ok) expect(out.error).toBe('not-signed-in');
  });

  test('NEGATIVE PROOF: a well-formed key alone is not sufficient', () => {
    // The bind must never succeed on the strength of the key. If this ever
    // returns ok, anyone who can POST can speak as anyone.
    expect(decideBind(null, PUBKEY).ok).toBe(false);
    expect(decideBind('', PUBKEY).ok).toBe(false);
  });
});

describe('#3827 what counts as a key', () => {
  test.each([
    ['too short', 'abc'],
    ['not hex', 'z'.repeat(64)],
    ['65 chars', `${'a'.repeat(65)}`],
    ['empty', ''],
    ['missing', undefined],
    ['an object', { pubkey: PUBKEY }],
  ])('refuses %s as bad-pubkey', (_label, value) => {
    const out = decideBind(WEBID, value);
    expect(out.ok).toBe(false);
    if (!out.ok) {
      expect(out.error).toBe('bad-pubkey');
      expect(out.status).toBe(400);
    }
  });

  test('NEGATIVE PROOF: the shape check cannot tell a private key from a public one', () => {
    // Both are 32 bytes, so this test documents a limit rather than a
    // guarantee: if a private key ever arrives here it will look valid. That is
    // why the browser sends only the public half and why this endpoint stores
    // into a PUBLIC binding — the protection is the design, not the regex.
    // Asserting the opposite would be a false comfort.
    const privateLookingKey = 'f'.repeat(64);
    expect(decideBind(WEBID, privateLookingKey).ok).toBe(true);
  });

  test('a key is normalised so the same key never binds twice under two spellings', () => {
    const upper = decideBind(WEBID, PUBKEY.toUpperCase());
    const lower = decideBind(WEBID, PUBKEY.toLowerCase());
    expect(upper.ok && lower.ok && upper.pubkey === lower.pubkey).toBe(true);
  });
});
