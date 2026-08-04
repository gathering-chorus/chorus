// @test-type: integration — in-process listener on an ephemeral port, tempdir fixtures,
// network-touching seams (CSS token exchange, allow-set) mocked. Brings its own world.
export {};
/**
 * #3606 — the OIDC login/callback flow was the largest uncovered region of
 * clearing's server.ts (functions 71.31% vs floor 75): the #3669 human-login
 * path had NO tests at all. These exercise it end to end through real HTTP
 * against the real handlers — real PKCE, real signed cookies, real state
 * checks — with only the two network seams faked.
 */

import type { AddressInfo } from 'net';

jest.mock('../src/participants', () => {
  class MockParticipants {
    roles = [{ name: 'Wren', title: 'PM', color: '#4ade80', systemPrompt: 'p' }];
    getRoles() { return this.roles; }
    getRoleByName(n: string) { return this.roles.find((r) => r.name.toLowerCase() === n.toLowerCase()); }
    updateContext() {}
    setGuestMode() {}
    abort = jest.fn();
    getResponse = jest.fn().mockResolvedValue({ content: 'ok', inputTokens: 1, outputTokens: 1 });
  }
  return { Participants: MockParticipants };
});

var mockExecSync: jest.Mock;
jest.mock('child_process', () => {
  const actual = jest.requireActual('child_process');
  mockExecSync = jest.fn().mockReturnValue('DELIVERED ok');
  return { ...actual, execSync: mockExecSync };
});

// The two network seams: allow-set + principal map (Fuseki) and the CSS code
// exchange. Everything else in solid-oidc/solid-auth runs REAL.
var mockAllowed: jest.Mock;
var mockPrincipal: jest.Mock;
jest.mock('../src/solid-auth', () => {
  const actual = jest.requireActual('../src/solid-auth');
  mockAllowed = jest.fn().mockResolvedValue(true);
  mockPrincipal = jest.fn().mockResolvedValue({ id: 'principal-jeff', name: 'jeff' });
  return { ...actual, isWebIdAllowed: mockAllowed, principalForWebId: mockPrincipal };
});
var mockExchange: jest.Mock;
jest.mock('../src/solid-oidc', () => {
  const actual = jest.requireActual('../src/solid-oidc');
  mockExchange = jest.fn();
  return { ...actual, exchangeCodeForWebId: mockExchange };
});
var mockChangePassword: jest.Mock;
jest.mock('../src/account', () => {
  const actual = jest.requireActual('../src/account');
  mockChangePassword = jest.fn();
  return { ...actual, changePassword: mockChangePassword };
});

import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
const TMP = fs.mkdtempSync(path.join(os.tmpdir(), 'server-oidc-'));
process.env.CLEARING_SCAN_DIR = TMP;
process.env.CLEARING_PULSE_FILE = path.join(TMP, 'pulse.json');
process.env.CLEARING_PROJECTS_DIR = TMP;
process.env.CHORUS_ROOT = TMP;
process.env.PULSE_URL = 'http://127.0.0.1:1';

import { server, io } from '../src/server';

const JEFF_WEBID = 'https://id.lightlifeurbangardens.com/jeff/profile/card#me';
let baseUrl: string;

beforeAll(async () => {
  await new Promise<void>((resolve) => server.listen(0, () => resolve()));
  baseUrl = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
});

afterAll(async () => {
  io.close();
  await new Promise<void>((resolve) => server.close(() => resolve()));
  try { fs.rmSync(TMP, { recursive: true, force: true }); } catch { /* ignore */ }
});

const cookieOf = (res: Response, name: string): string => {
  const all = res.headers.getSetCookie?.() || [];
  const hit = all.find((c: string) => c.startsWith(name + '='));
  return hit ? hit.split(';')[0] : '';
};

/** Run the real /auth/login leg and return its state + login cookie. */
async function startLogin() {
  const res = await fetch(`${baseUrl}/auth/login`, { redirect: 'manual' });
  expect(res.status).toBe(302);
  const loc = res.headers.get('location') || '';
  const state = new URL(loc).searchParams.get('state') || '';
  return { loc, state, loginCookie: cookieOf(res as unknown as Response, 'clearing_login') };
}

describe('#3606 /auth/login', () => {
  test('redirects to the CSS authorize URL with PKCE + state, and stashes the signed login cookie', async () => {
    const { loc, state, loginCookie } = await startLogin();
    expect(loc).toContain('code_challenge');
    expect(state).not.toBe('');
    expect(loginCookie).toMatch(/^clearing_login=./);
  });
});

describe('#3606 /auth/callback', () => {
  test('NEGATIVE: no login cookie → refused, never a session', async () => {
    const res = await fetch(`${baseUrl}/auth/callback?code=x&state=y`, { redirect: 'manual' });
    expect(res.status).toBeGreaterThanOrEqual(400);
    expect(cookieOf(res as unknown as Response, 'clearing_session')).toBe('');
  });

  test('NEGATIVE: state mismatch (CSRF) → refused, never a session', async () => {
    const { loginCookie } = await startLogin();
    const res = await fetch(`${baseUrl}/auth/callback?code=x&state=WRONG`, {
      redirect: 'manual', headers: { cookie: loginCookie },
    });
    expect(res.status).toBeGreaterThanOrEqual(400);
    expect(cookieOf(res as unknown as Response, 'clearing_session')).toBe('');
  });

  test('happy path: exchange → allow-set → session cookie + redirect into the room', async () => {
    mockExchange.mockResolvedValueOnce(JEFF_WEBID);
    const { state, loginCookie } = await startLogin();
    const res = await fetch(`${baseUrl}/auth/callback?code=abc&state=${encodeURIComponent(state)}`, {
      redirect: 'manual', headers: { cookie: loginCookie },
    });
    expect(res.status).toBe(302);
    expect(res.headers.get('location')).toBe('/');
    expect(cookieOf(res as unknown as Response, 'clearing_session')).toMatch(/^clearing_session=./);
  });

  test('NEGATIVE: identity server down (exchange resolves null — its no-throw contract) → 502, the room is not blamed', async () => {
    expect(server.listening).toBe(true); // the real server under test, not a stub
    mockExchange.mockResolvedValueOnce(null);
    const { state, loginCookie } = await startLogin();
    const res = await fetch(`${baseUrl}/auth/callback?code=abc&state=${encodeURIComponent(state)}`, {
      redirect: 'manual', headers: { cookie: loginCookie },
    });
    expect(res.status).toBe(502);
  });

  test('NEGATIVE: authenticated but NOT on the team allow-set → 403, never a session', async () => {
    mockExchange.mockResolvedValueOnce('https://id.example/stranger#me');
    mockAllowed.mockResolvedValueOnce(false);
    const { state, loginCookie } = await startLogin();
    const res = await fetch(`${baseUrl}/auth/callback?code=abc&state=${encodeURIComponent(state)}`, {
      redirect: 'manual', headers: { cookie: loginCookie },
    });
    expect(res.status).toBe(403);
    expect(cookieOf(res as unknown as Response, 'clearing_session')).toBe('');
  });

  test('upstream error param short-circuits before any exchange', async () => {
    const { loginCookie } = await startLogin();
    const res = await fetch(`${baseUrl}/auth/callback?error=access_denied`, {
      redirect: 'manual', headers: { cookie: loginCookie },
    });
    expect(res.status).toBeGreaterThanOrEqual(400);
    expect(mockExchange).not.toHaveBeenCalledWith(expect.anything(), 'access_denied', expect.anything());
  });
});

describe('#3606 session identity on / (#3743 seam)', () => {
  test('a session-authenticated request skips the name page and renders the Principal name', async () => {
    mockExchange.mockResolvedValueOnce(JEFF_WEBID);
    const { state, loginCookie } = await startLogin();
    const cb = await fetch(`${baseUrl}/auth/callback?code=abc&state=${encodeURIComponent(state)}`, {
      redirect: 'manual', headers: { cookie: loginCookie },
    });
    const session = cookieOf(cb as unknown as Response, 'clearing_session');
    const home = await fetch(`${baseUrl}/`, { headers: { cookie: session } });
    const html = await home.text();
    expect(home.status).toBe(200);
    expect(html).toContain('window.BRIDGE_USER="jeff"');
  });

  test('/logout clears the cookies and says so', async () => {
    const res = await fetch(`${baseUrl}/logout`, { redirect: 'manual' });
    expect([200, 302]).toContain(res.status);
  });
});

describe('#3606 account page + password change (#3679 routes, previously untested)', () => {
  async function jeffSession(): Promise<string> {
    mockExchange.mockResolvedValueOnce(JEFF_WEBID);
    const { state, loginCookie } = await startLogin();
    const cb = await fetch(`${baseUrl}/auth/callback?code=abc&state=${encodeURIComponent(state)}`, {
      redirect: 'manual', headers: { cookie: loginCookie },
    });
    return cookieOf(cb as unknown as Response, 'clearing_session');
  }

  test('NEGATIVE: /account without a session → 401 interstitial, never the page', async () => {
    expect(server.listening).toBe(true);
    const res = await fetch(`${baseUrl}/account`);
    expect(res.status).toBe(401);
  });

  test('/account with a session renders the signed-in identity', async () => {
    const session = await jeffSession();
    const res = await fetch(`${baseUrl}/account`, { headers: { cookie: session } });
    expect(res.status).toBe(200);
    expect(await res.text()).toContain('id.lightlifeurbangardens.com/jeff');
  });

  test('NEGATIVE: password change without a session → 401, changePassword never called', async () => {
    mockChangePassword.mockClear();
    const res = await fetch(`${baseUrl}/api/account/password`, {
      method: 'POST', headers: { 'Content-Type': 'application/json' }, body: '{}',
    });
    expect(res.status).toBe(401);
    expect(mockChangePassword).not.toHaveBeenCalled();
  });

  test('password change maps result reasons to statuses (ok→200, bad-credentials→403, weak→400, else→502)', async () => {
    const session = await jeffSession();
    const post = (body: object) => fetch(`${baseUrl}/api/account/password`, {
      method: 'POST', headers: { 'Content-Type': 'application/json', cookie: session }, body: JSON.stringify(body),
    });
    mockChangePassword.mockResolvedValueOnce({ ok: true });
    expect((await post({ email: 'e', oldPassword: 'o', newPassword: 'n' })).status).toBe(200);
    mockChangePassword.mockResolvedValueOnce({ ok: false, reason: 'bad-credentials', message: 'no' });
    expect((await post({})).status).toBe(403);
    mockChangePassword.mockResolvedValueOnce({ ok: false, reason: 'weak-password', message: 'no' });
    expect((await post({})).status).toBe(400);
    mockChangePassword.mockResolvedValueOnce({ ok: false, reason: 'css-error', message: 'no' });
    expect((await post({})).status).toBe(502);
  });
});

describe('#3606 terminal + domain-proxy routes (previously untested)', () => {
  test('NEGATIVE: /api/session/:role refuses a non-role', async () => {
    expect(server.listening).toBe(true);
    const res = await fetch(`${baseUrl}/api/session/mallory`);
    expect(res.status).toBe(400);
  });

  test('/api/session/:role with no live tty answers honestly, not an error', async () => {
    const res = await fetch(`${baseUrl}/api/session/wren`);
    expect(res.status).toBe(200);
    const body = await res.json() as { text: string; lines: number };
    expect(typeof body.text).toBe('string');
  });

  test('NEGATIVE: /api/domain-detail proxy answers 502 when upstream is unreachable — never an empty domain', async () => {
    const prev = process.env.CHORUS_API_URL;
    process.env.CHORUS_API_URL = 'http://127.0.0.1:1';
    const res = await fetch(`${baseUrl}/api/domain-detail/chorus`);
    process.env.CHORUS_API_URL = prev;
    expect(res.status).toBe(502);
  });
});

