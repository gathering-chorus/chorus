// @test-type: security
/**
 * #3679 — self-service password change. Pins the security-critical logic:
 * session-binding (you can only change the identity you're signed in as), fail-closed
 * ordering (no mutation before the ownership check), enumeration-safety (one generic
 * error for wrong-password AND no-such-account), and no-secret-in-logs. CSS is mocked.
 */

import { changePassword, sameWebid } from '../src/account';

const CSS = 'http://localhost:3001';
const MARK = 'https://id.lightlifeurbangardens.com/marknakib/profile/card#me';
const JEFF = 'https://id.lightlifeurbangardens.com/jeff/profile/card#me';
const EMAIL = 'marknakib@gmail.com';
const CHANGE_URL = 'https://id.lightlifeurbangardens.com/.account/account/ed70/login/password/65e2/';

/** A configurable CSS mock. `linksFor` decides which WebIDs the authed account owns. */
function mockCss(opts: { loginOk?: boolean; linkedWebid?: string; hasLogin?: boolean; changeOk?: boolean } = {}) {
  const { loginOk = true, linkedWebid = MARK, hasLogin = true, changeOk = true } = opts;
  const calls: string[] = [];
  const fetchImpl = (async (url: string, init?: RequestInit) => {
    const u = String(url);
    const method = init?.method ?? 'GET';
    calls.push(`${method} ${u.replace(CSS, '')}`);
    const json = (body: unknown, ok = true, status = ok ? 200 : 400) =>
      ({ ok, status, json: async () => body }) as unknown as Response;

    if (u.endsWith('/.account/login/password/') && method === 'POST') {
      return loginOk ? json({ authorization: 'acct-token' }) : json({ error: 'invalid' }, false, 401);
    }
    if (u.endsWith('/.account/')) {
      return json({ controls: { account: { webId: 'https://id.lightlifeurbangardens.com/.account/account/ed70/webid/' },
                                password: { create: 'https://id.lightlifeurbangardens.com/.account/account/ed70/login/password/' } } });
    }
    if (u.endsWith('/webid/')) return json({ webIdLinks: { [linkedWebid]: 'x' } });
    if (u.endsWith('/login/password/') && method === 'GET') {
      return json({ passwordLogins: hasLogin ? { [EMAIL]: CHANGE_URL } : {} });
    }
    if (u === CHANGE_URL.replace(/^https?:\/\/[^/]+/, CSS) && method === 'POST') {
      return changeOk ? json({}) : json({ error: 'refused' }, false, 400);
    }
    return json({ error: 'unexpected' }, false, 404);
  }) as unknown as typeof fetch;
  return { fetchImpl, calls };
}

const P = { sessionWebid: MARK, email: EMAIL, oldPassword: 'currentpw123', newPassword: 'brandnewpw456' };

describe('#3679 changePassword — security core', () => {
  test('weak new password is refused BEFORE any CSS call', async () => {
    const { fetchImpl, calls } = mockCss();
    const r = await changePassword({ ...P, newPassword: 'short' }, CSS, fetchImpl);
    expect(r).toEqual({ ok: false, reason: 'weak-password', message: expect.any(String) });
    expect(calls).toEqual([]); // nothing hit CSS
  });

  test('success: session owns the account → change POST fires and returns ok', async () => {
    const { fetchImpl, calls } = mockCss({ linkedWebid: MARK });
    const r = await changePassword(P, CSS, fetchImpl);
    expect(r).toEqual({ ok: true });
    expect(calls[calls.length - 1]).toBe(`POST ${CHANGE_URL.replace(/^https?:\/\/[^/]+/, '')}`);
  });

  test('BINDING: session WebID not among the account links → not-your-account, and NO mutation', async () => {
    // authenticated as an account whose only link is JEFF, but the session is MARK
    const { fetchImpl, calls } = mockCss({ linkedWebid: JEFF });
    const r = await changePassword({ ...P, sessionWebid: MARK }, CSS, fetchImpl);
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.reason).toBe('not-your-account');
    expect(calls.some((c) => c.startsWith(`POST ${CHANGE_URL.replace(/^https?:\/\/[^/]+/, '')}`))).toBe(false); // fail-closed: never changed
  });

  test('wrong current password → generic bad-credentials, no mutation', async () => {
    const { fetchImpl, calls } = mockCss({ loginOk: false });
    const r = await changePassword(P, CSS, fetchImpl);
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.reason).toBe('bad-credentials');
    expect(calls).toEqual(['POST /.account/login/password/']); // stopped at auth
  });

  test('ENUMERATION-SAFE: no-such-account returns the SAME message as wrong-password', async () => {
    // CSS returns 401 for both a wrong password and an unknown email — the route must not distinguish
    const wrongPw = await changePassword(P, CSS, mockCss({ loginOk: false }).fetchImpl);
    const noSuchAcct = await changePassword({ ...P, email: 'ghost@nobody.com' }, CSS, mockCss({ loginOk: false }).fetchImpl);
    expect(wrongPw.ok).toBe(false); expect(noSuchAcct.ok).toBe(false);
    if (!wrongPw.ok && !noSuchAcct.ok) expect(wrongPw.message).toBe(noSuchAcct.message); // no oracle
  });

  test('CSS refuses the change (e.g. policy) → clean css-error, no secret in message', async () => {
    const { fetchImpl } = mockCss({ changeOk: false });
    const r = await changePassword(P, CSS, fetchImpl);
    expect(r.ok).toBe(false);
    if (!r.ok) { expect(r.reason).toBe('css-error'); expect(r.message).not.toContain(P.oldPassword); expect(r.message).not.toContain(P.newPassword); }
  });

  test('NO SECRET IN LOGS: neither password appears in any console output across success + failure', async () => {
    const spy = jest.spyOn(console, 'log').mockImplementation(() => {});
    const errSpy = jest.spyOn(console, 'error').mockImplementation(() => {});
    await changePassword(P, CSS, mockCss().fetchImpl);
    await changePassword(P, CSS, mockCss({ loginOk: false }).fetchImpl);
    const logged = [...spy.mock.calls, ...errSpy.mock.calls].flat().map(String).join(' ');
    expect(logged).not.toContain(P.oldPassword);
    expect(logged).not.toContain(P.newPassword);
    spy.mockRestore(); errSpy.mockRestore();
  });

  test('sameWebid ignores #me fragment and trailing slash', () => {
    expect(sameWebid('https://x/a/profile/card#me', 'https://x/a/profile/card')).toBe(true);
    expect(sameWebid('https://x/a/profile/card#me', 'https://x/b/profile/card#me')).toBe(false);
    expect(sameWebid('', 'https://x/a')).toBe(false);
  });
});
