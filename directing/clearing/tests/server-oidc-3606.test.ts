// @test-type: integration — in-process listener on an ephemeral port, tempdir fixtures,
// @card: #3606
// @owner: wren
// network-touching seams (CSS token exchange, allow-set) mocked. Brings its own world.
/**
 * #3606 — routes of clearing's server.ts that had no tests, driven through real
 * HTTP with only the network seams faked. #4412 deleted the Clearing's own
 * OIDC sign-in (one sign-in for Chorus: the door), so the session here is the
 * door's signed cookie, minted with a temp key.
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

// The network seam: allow-set + principal map (Fuseki). The rest of solid-auth runs REAL.
var mockAllowed: jest.Mock;
var mockPrincipal: jest.Mock;
jest.mock('../src/solid-auth', () => {
  const actual = jest.requireActual('../src/solid-auth');
  mockAllowed = jest.fn().mockResolvedValue(true);
  mockPrincipal = jest.fn().mockResolvedValue({ id: 'principal-jeff', name: 'jeff' });
  return { ...actual, isWebIdAllowed: mockAllowed, principalForWebId: mockPrincipal };
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
import * as crypto from 'crypto';
const TMP = fs.mkdtempSync(path.join(os.tmpdir(), 'server-oidc-'));
process.env.CLEARING_SCAN_DIR = TMP;
process.env.CLEARING_PULSE_FILE = path.join(TMP, 'pulse.json');
process.env.CLEARING_PROJECTS_DIR = TMP;
process.env.CHORUS_ROOT = TMP;
process.env.PULSE_URL = 'http://127.0.0.1:1';
// The door's key: share-session reads SHARE_STATE_FILE on each verify.
const KEY = crypto.randomBytes(32);
const b64u = (b: Buffer): string => b.toString('base64').replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
fs.writeFileSync(process.env.SHARE_STATE_FILE!, JSON.stringify({ cookie_key: b64u(KEY) }));

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

/** The door's signed session cookie for a WebID (what chorus-share-guard.py mints). */
function doorSession(webid: string): string {
  const payload = b64u(Buffer.from(JSON.stringify({ webid, exp: Math.floor(Date.now() / 1000) + 600 })));
  return `chorus_share_session=${payload}.${b64u(crypto.createHmac('sha256', KEY).update(payload).digest())}`;
}

describe('#3606 session identity on / (#3743 seam)', () => {
  test('a door-session request skips the name page and renders the Principal name', async () => {
    const home = await fetch(`${baseUrl}/`, { headers: { cookie: doorSession(JEFF_WEBID) } });
    const html = await home.text();
    expect(home.status).toBe(200);
    expect(html).toContain('window.BRIDGE_USER="jeff"');
  });

  test('/logout drops the bridge cookies and signs out at the door (#4412)', async () => {
    const res = await fetch(`${baseUrl}/logout`, { redirect: 'manual' });
    expect(res.status).toBe(302);
    expect(res.headers.get('location')).toBe('https://chorus.lightlifeurbangardens.com/_auth/logout');
  });
});

describe('#3606 account page + password change (#3679 routes, previously untested)', () => {
  async function jeffSession(): Promise<string> {
    return doorSession(JEFF_WEBID);
  }

  test('NEGATIVE: /account without a session → sent to the door (503 when none is configured), never the page', async () => {
    expect(server.listening).toBe(true);
    delete process.env.CHORUS_SIGNIN_URL;
    const res = await fetch(`${baseUrl}/account`);
    expect(res.status).toBe(503);
    expect(await res.text()).not.toContain('Your account');
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

