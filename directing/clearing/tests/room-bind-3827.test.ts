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

import fs from 'fs';
import path from 'path';
import { decideBind } from '../src/room-bind';

const WEBID = 'https://id.lightlifeurbangardens.com/jeff/profile/card#me';
const PUBKEY = 'a1b2c3d4'.repeat(8); // 64 hex chars

describe('#3827 bind requires a signed-in identity', () => {
  test('a signed-in WebID with a well-formed key binds', () => {
    // Asserted on the whole object rather than behind an `if (out.ok)` guard:
    // a conditional expect silently passes when the condition is false, which
    // is the same "green because it never ran" shape this suite exists to
    // prevent.
    expect(decideBind(WEBID, PUBKEY)).toEqual({
      ok: true, webid: WEBID, pubkey: PUBKEY.toLowerCase(),
    });
  });

  test('an unauthenticated caller is refused, and the refusal names the state', () => {
    // "not signed in" is a different situation from "bad key". A refusal that
    // says only "refused" sends the reader to the wrong half of the system —
    // precisely how the relay's "must include an h tag" cost an hour this
    // morning when it meant "you are not a member".
    const out = decideBind(null, PUBKEY);
    expect(out).toMatchObject({ ok: false, error: 'not-signed-in', status: 401 });
    expect(JSON.stringify(out)).toMatch(/sign in/i);
  });

  test('NEGATIVE PROOF: identity is checked BEFORE the key', () => {
    // If the key were validated first, an unauthenticated caller sending
    // garbage would be told "bad-pubkey" — which tells them their session was
    // accepted and only the key was wrong. Both wrong at once must still fail
    // on identity.
    expect(decideBind(null, 'not-a-key')).toMatchObject({ ok: false, error: 'not-signed-in' });
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
    expect(decideBind(WEBID, value)).toMatchObject({ ok: false, error: 'bad-pubkey', status: 400 });
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

describe('#3827 the page actually joins', () => {
  // The first version of room-key.js exported join() and nothing called it.
  // Everything was correct and nothing happened: Jeff opened the room over and
  // over and the bindings stayed empty, which reads as "my key failed" rather
  // than "nobody asked". These assert the WIRING, because that is what was
  // missing — not the crypto.
  const roomKeySrc = fs.readFileSync(path.join(__dirname, '..', 'public', 'room-key.js'), 'utf8');
  const indexSrc = fs.readFileSync(path.join(__dirname, '..', 'public', 'index.html'), 'utf8');

  test('room-key.js invokes the join itself, not just defines it', () => {
    expect(roomKeySrc).toMatch(/autoJoin/);
    // An IIFE — defined AND called. `function autoJoin(){}` alone would satisfy
    // a naive "is it there" check while doing nothing, which is the exact bug.
    expect(roomKeySrc).toMatch(/\(async function autoJoin\(\)[\s\S]*\}\(\)\);/);
  });

  test('the page loads the module', () => {
    expect(indexSrc).toMatch(/<script[^>]+src="\/room-key\.js"/);
  });

  test('NEGATIVE PROOF: signed out, no key is minted', () => {
    // Minting for an anonymous visitor would create an identity belonging to
    // nobody that looks exactly like one belonging to Jeff. The guard is the
    // empty-WebID early return.
    expect(roomKeySrc).toMatch(/if \(!webid\)[\s\S]{0,200}signed-out/);
  });

  test('NEGATIVE PROOF: a failed join is reported, never swallowed', () => {
    // A silent catch is indistinguishable from never having tried — the shape
    // of this very defect. The catch must record a state.
    expect(roomKeySrc).toMatch(/state: 'failed'/);
  });

  test('the WebID reaches the page from the SERVER session, not the client', () => {
    const serverSrc = fs.readFileSync(path.join(__dirname, '..', 'src', 'server.ts'), 'utf8');
    expect(serverSrc).toMatch(/window\.CHORUS_WEBID/);
    // sessionWebid() reads the signed cookie. If this ever came from a request
    // body or query param, anyone could claim to be anyone (#3679).
    expect(serverSrc).toMatch(/const sessionWebId = sessionWebid\(req\)/);
  });
});
