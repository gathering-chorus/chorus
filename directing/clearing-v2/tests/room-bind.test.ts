// @test-type: unit — calls the pure bind decision directly; no server, no relay, no pod.
// @card: #4530
// @owner: abby-normal
// @domain: messages

import { MessageRouter } from '../src/router';
import { decideBind } from '../src/room-bind';

const WEBID = 'https://id.lightlifeurbangardens.com/jeff/profile/card#me';
const PUBKEY = 'a1b2c3d4'.repeat(8); // 64 hex chars

describe('#3827/#4530 bind requires a signed-in identity', () => {
  test('a signed-in WebID with a well-formed key binds', () => {
    const router = new MessageRouter();
    expect(decideBind(WEBID, PUBKEY)).toEqual({
      ok: true, webid: WEBID, pubkey: PUBKEY.toLowerCase(),
    });
    expect(router.getRecent(1)).toEqual([]);
  });

  test('an unauthenticated caller is refused, and the refusal names the state', () => {
    const router = new MessageRouter();
    const out = decideBind(null, PUBKEY);
    expect(out).toMatchObject({ ok: false, error: 'not-signed-in', status: 401 });
    expect(JSON.stringify(out)).toMatch(/sign in/i);
    expect(router.getRecent(1)).toEqual([]);
  });

  test('NEGATIVE PROOF: identity is checked BEFORE the key', () => {
    const router = new MessageRouter();
    expect(decideBind(null, 'not-a-key')).toMatchObject({ ok: false, error: 'not-signed-in' });
    expect(router.getRecent(1)).toEqual([]);
  });

  test('NEGATIVE PROOF: a well-formed key alone is not sufficient', () => {
    const router = new MessageRouter();
    expect(decideBind(null, PUBKEY).ok).toBe(false);
    expect(decideBind('', PUBKEY).ok).toBe(false);
    expect(router.getRecent(1)).toEqual([]);
  });
});

describe('#3827/#4530 what counts as a key', () => {
  test.each([
    ['too short', 'abc'],
    ['not hex', 'z'.repeat(64)],
    ['65 chars', `${'a'.repeat(65)}`],
    ['empty', ''],
    ['missing', undefined],
    ['an object', { pubkey: PUBKEY }],
  ])('refuses %s as bad-pubkey', (_label, value) => {
    const router = new MessageRouter();
    expect(decideBind(WEBID, value)).toMatchObject({ ok: false, error: 'bad-pubkey', status: 400 });
    expect(router.getRecent(1)).toEqual([]);
  });

  test('a key is normalised so the same key never binds twice under two spellings', () => {
    const router = new MessageRouter();
    const upper = decideBind(WEBID, PUBKEY.toUpperCase());
    const lower = decideBind(WEBID, PUBKEY.toLowerCase());
    expect(upper.ok && lower.ok && upper.pubkey === lower.pubkey).toBe(true);
    expect(router.getRecent(1)).toEqual([]);
  });
});
