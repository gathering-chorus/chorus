// @test-type: integration — boots the real app on an ephemeral port; simulates
// the tunnel with a cf-ray header (isLocal classifies by header). Token comes
// from ~/.chorus/bridge-auth-token — the same pre-existing machine coupling
// server.ts itself has (named boundary, not extended).
/**
 * #3667 — wire-level tunnel auth matrix.
 *
 * Red on pre-fix behavior: /api/stream and /api/flow hard-403 any cf-proxied
 * request before the token is read, and /api/domain-detail/ does not exist.
 */

import type { AddressInfo } from 'net';

jest.mock('../src/participants', () => {
  class MockParticipants {
    roles = [
      { name: 'Wren', title: 'PM', color: '#4ade80', systemPrompt: 'p' },
      { name: 'Silas', title: 'Arch', color: '#60a5fa', systemPrompt: 'p' },
      { name: 'Kade', title: 'Eng', color: '#fb923c', systemPrompt: 'p' },
    ];
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

import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import * as http from 'http';

const TMP = fs.mkdtempSync(path.join(os.tmpdir(), 'tunnel-auth-test-'));
process.env.CLEARING_SCAN_DIR = TMP;
process.env.CLEARING_PULSE_FILE = path.join(TMP, 'pulse.json');
process.env.CLEARING_PROJECTS_DIR = TMP;
process.env.CHORUS_ROOT = TMP;
process.env.PULSE_URL = 'http://127.0.0.1:1';

// Stub upstream for the domain-detail proxy — the test brings its own :3340.
let stubUpstream: http.Server;
let stubHits: string[] = [];

import { server, io, tailer, sessionTailer } from '../src/server';

let baseUrl: string;
const TOKEN = fs.readFileSync(path.join(os.homedir(), '.chorus', 'bridge-auth-token'), 'utf-8').trim();

beforeAll(async () => {
  stubUpstream = http.createServer((req, res) => {
    stubHits.push(req.url || '');
    res.setHeader('content-type', 'application/json');
    res.end(JSON.stringify({ domain: 'chorus', cards: 7 }));
  });
  await new Promise<void>((resolve) => stubUpstream.listen(0, '127.0.0.1', () => resolve()));
  process.env.CHORUS_API_URL = `http://127.0.0.1:${(stubUpstream.address() as AddressInfo).port}`;

  await new Promise<void>((resolve) => server.listen(0, () => resolve()));
  baseUrl = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
});

afterAll(async () => {
  tailer.stop();
  sessionTailer.stop();
  io.close();
  await new Promise<void>((resolve) => server.close(() => resolve()));
  await new Promise<void>((resolve) => stubUpstream.close(() => resolve()));
  try { fs.rmSync(TMP, { recursive: true, force: true }); } catch { /* ignore */ }
});

const TUNNEL = { 'cf-ray': '8abc123-BOS' };

async function call(p: string, headers: Record<string, string> = {}, method = 'GET') {
  const res = await fetch(`${baseUrl}${p}`, { method, headers, redirect: 'manual' });
  return res.status;
}

describe('tunnel auth matrix (#3667)', () => {
  test('tunneled GET /api/flow with token → 200 (Domains tab)', async () => {
    expect(await call('/api/flow', { ...TUNNEL, authorization: `Bearer ${TOKEN}` })).toBe(200);
  });

  test('tunneled GET /api/stream with token → 200 (Streams tab)', async () => {
    expect(await call('/api/stream?lines=5', { ...TUNNEL, authorization: `Bearer ${TOKEN}` })).toBe(200);
  });

  test('tunneled GET read pair without token → 401 login, not hard 403', async () => {
    expect(await call('/api/flow', TUNNEL)).toBe(401);
    expect(await call('/api/stream?lines=5', TUNNEL)).toBe(401);
  });

  test('tunneled admin trio with token → 403 always', async () => {
    expect(await call('/api/restart', { ...TUNNEL, authorization: `Bearer ${TOKEN}` })).toBe(403);
    expect(await call('/api/commands/kade', { ...TUNNEL, authorization: `Bearer ${TOKEN}` })).toBe(403);
    expect(await call('/api/session/wren', { ...TUNNEL, authorization: `Bearer ${TOKEN}` })).toBe(403);
  });

  test('tunneled POST to read pair with token → 403 (only GET opened)', async () => {
    expect(await call('/api/flow', { ...TUNNEL, authorization: `Bearer ${TOKEN}` }, 'POST')).toBe(403);
  });

  test('local /api/flow and /api/stream unchanged → 200', async () => {
    expect(await call('/api/flow')).toBe(200);
    expect(await call('/api/stream?lines=5')).toBe(200);
  });

  test('domain-detail proxy: tunneled GET with token → 200 via upstream stub, path forwarded', async () => {
    stubHits = [];
    const status = await call('/api/domain-detail/chorus', { ...TUNNEL, authorization: `Bearer ${TOKEN}` });
    expect(status).toBe(200);
    expect(stubHits).toEqual(['/api/chorus/domain/chorus']);
  });
});
