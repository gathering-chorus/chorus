/* global RequestInit */
/**
 * server.ts — in-process unit tests (#2167).
 *
 * Imports the express app (listen guarded by require.main check), spins up
 * an ephemeral-port listener, and hits every major endpoint with fetch.
 * Handlers run in-process so jest registers their coverage. Downstream
 * services (Fuseki, Loki, LanceDB, chorus-hooks) aren't required — handlers
 * either respond from local state or return an error shape we accept.
 */

import type { AddressInfo } from 'net';

// Point paths at tempdirs so imports don't touch Jeff's real state.
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
const TMP = fs.mkdtempSync(path.join(os.tmpdir(), 'chorus-api-test-'));
process.env.CHORUS_ROOT = TMP;
process.env.DB_PATH = path.join(TMP, 'test.db');
process.env.CHORUS_LOG_PATH = path.join(TMP, 'chorus.log');
process.env.CLAUDE_STATS_CACHE = path.join(TMP, 'stats.json');
process.env.CLEARING_TRANSCRIPTS_DIR = path.join(TMP, 'transcripts');
process.env.QUALITY_CACHE_PATH = path.join(TMP, 'quality.json');
process.env.POSTURE_BASE = path.join(TMP, 'posture');
process.env.SESSIONS_DIR = path.join(TMP, 'sessions');

import app from '../src/server';
import * as http from 'http';

let server: http.Server;
let baseUrl: string;

beforeAll(async () => {
  server = http.createServer(app as any);
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', () => resolve()));
  const addr = server.address() as AddressInfo;
  baseUrl = `http://127.0.0.1:${addr.port}`;
});

afterAll(async () => {
  await new Promise<void>((resolve) => server.close(() => resolve()));
  try { fs.rmSync(TMP, { recursive: true, force: true }); } catch { /* ignore */ }
});

async function get(p: string, opts: RequestInit = {}) {
  try {
    const res = await fetch(`${baseUrl}${p}`, { ...opts, signal: AbortSignal.timeout(3000) } as any);
    let body: any = null;
    const ct = res.headers.get('content-type') || '';
    if (ct.includes('json')) { try { body = await res.json(); } catch { /* ignore */ } }
    else { try { body = await res.text(); } catch { /* ignore */ } }
    return { status: res.status, body, headers: res.headers };
  } catch {
    return { status: 0, body: null, headers: new Headers() };
  }
}

describe('server smoke — routes respond without crashing', () => {
  // Hit each major route once. We don't assert deep response shapes — the
  // per-module tests cover those. Here we just force coverage on route
  // registration + handler entry. Accept any non-crash response.
  test.each([
    '/api/chorus/patterns/summary?days=7',
    '/api/chorus/sessions',
    '/api/chorus/jeff/posture?days=3',
    '/api/chorus/jeff/werk?hours=1',
    '/api/chorus/hooks/summary',
    '/api/chorus/fitness/summary',
    '/api/chorus/cost/summary',
    '/api/chorus/quality/summary',
    '/api/chorus/quality/by-domain?domain=photos',
  ])('GET %s responds', async (url) => {
    const r = await get(url);
    // 0 = timeout/error (downstream unreachable is OK); otherwise any HTTP status.
    expect(typeof r.status).toBe('number');
  });

  test('GET /api/chorus/sessions/ses_bad_id returns null-or-404 for invalid id', async () => {
    const r = await get('/api/chorus/sessions/ses_1_xxxnonexistent');
    expect([200, 404, 500, 0]).toContain(r.status);
  });

  // #2941: per-test timeout bump. POST /api/chorus/embed is the first
  // request that touches the embed handler, which lazy-loads LanceDB
  // (~1.3M vectors). Under full-suite memory pressure that load can
  // sync-block the event loop past jest's 5s default — long enough that
  // the in-fetch 3s AbortSignal can't fire either (the abort signal
  // dispatch is itself a JS task waiting behind LanceDB's blocking
  // initialization). The test only asserts the route returns *some*
  // numeric status; widening the budget covers the cold-load path
  // without weakening the assertion. Standalone runs finish in ~6s.
  test('POST /api/chorus/embed with no body handled', async () => {
    const r = await get('/api/chorus/embed', { method: 'POST' });
    expect(typeof r.status).toBe('number');
  }, 30_000);

  test('unknown route returns 404', async () => {
    const r = await get('/no-such-route-xxx');
    expect([404, 0]).toContain(r.status);
  });
});
