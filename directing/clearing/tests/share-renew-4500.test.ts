// @test-type: unit:security — drives relayRenew with a stub guard (no network, fixed clock); brings its own world.
// @card: #4500
// @owner: wren
/**
 * #4500 — Jeff, 2026-10-12: "if i go to sleep and then return when i wake i
 * typically must login again". He uses the Clearing, which the guard never
 * sees, so the Clearing relays his cookie to the guard's renew route and hands
 * the renewed cookie back. These cases pin the relay: it asks at most once per
 * window per session, passes the renewed cookie through, and never invents one.
 */
import { relayRenew, RENEW_EVERY_MS, verifyShareSession } from '../src/share-session';
import crypto from 'crypto';

const T = 1_791_700_000_000;
const COOKIE = 'chorus_share_session=abc.def';

function guard(status: number, setCookie: string | null) {
  const calls: Array<{ url: string; cookie: string | undefined }> = [];
  const impl = (async (url: string, init?: { headers?: Record<string, string> }) => {
    calls.push({ url, cookie: init?.headers?.cookie });
    return { status, headers: { get: (h: string) => (h.toLowerCase() === 'set-cookie' ? setCookie : null) } };
  }) as unknown as typeof fetch;
  return { calls, impl };
}

describe('#4500 the Clearing keeps his shared-door sign-in alive', () => {
  it('relays his cookie to the guard renew route and passes the renewed cookie back', async () => {
    const g = guard(204, 'chorus_share_session=new.cookie; Path=/');
    const got = await relayRenew(COOKIE, 'jeff-browser-1', new Map(), 'http://127.0.0.1:8899/', T, g.impl);
    expect(got).toBe('chorus_share_session=new.cookie; Path=/');
    expect(g.calls).toEqual([{ url: 'http://127.0.0.1:8899/_auth/renew', cookie: COOKIE }]);
  });

  it('asks at most once per window per session, then again after it', async () => {
    const g = guard(204, null);
    const asked = new Map<string, number>();
    await relayRenew(COOKIE, 'jeff-browser-1', asked, 'http://g', T, g.impl);
    await relayRenew(COOKIE, 'jeff-browser-1', asked, 'http://g', T + RENEW_EVERY_MS - 1, g.impl);
    await relayRenew(COOKIE, 'jeff-browser-2', asked, 'http://g', T + 1, g.impl);
    await relayRenew(COOKIE, 'jeff-browser-1', asked, 'http://g', T + RENEW_EVERY_MS, g.impl);
    expect(g.calls).toHaveLength(3);
  });

  it('NEGATIVE PROOF: a refused renewal (signed out, revoked) passes no cookie through', async () => {
    const g = guard(401, 'chorus_share_session=should.not.pass');
    expect(await relayRenew(COOKIE, 'jeff-browser-1', new Map(), 'http://g', T, g.impl)).toBeNull();
  });

  it('NEGATIVE PROOF: a cookie with no session id is never relayed; a guard that is down never throws', async () => {
    const g = guard(204, 'x');
    expect(await relayRenew(COOKIE, undefined, new Map(), 'http://g', T, g.impl)).toBeNull();
    expect(g.calls).toHaveLength(0);
    const down = (async () => { throw new Error('ECONNREFUSED'); }) as unknown as typeof fetch;
    expect(await relayRenew(COOKIE, 'jeff-browser-1', new Map(), 'http://g', T, down)).toBeNull();
  });

  it('the verifier reads the session id the guard now signs, and still accepts a cookie without one', () => {
    const key = Buffer.from('test-key-4500-not-the-live-one-----');
    const sign = (claims: object) => {
      const p = Buffer.from(JSON.stringify(claims)).toString('base64url');
      return `${p}.${crypto.createHmac('sha256', key).update(p).digest('base64url')}`;
    };
    const now = 1_791_700_000;
    const withSid = verifyShareSession(sign({ webid: 'w', exp: now + 60, iat: now, sid: 'jeff-browser-1' }), key, now);
    const without = verifyShareSession(sign({ webid: 'w', exp: now + 60 }), key, now);
    expect(withSid).toEqual({ ok: true, webid: 'w', exp: now + 60, sid: 'jeff-browser-1' });
    expect(without).toEqual({ ok: true, webid: 'w', exp: now + 60 });
  });
});
