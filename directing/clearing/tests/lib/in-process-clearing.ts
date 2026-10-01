// @test-type: unit — fixture helper, not a suite
/**
 * #4417 — the Clearing's gate, run in-process for HTTP tests. A temp guard key
 * (SHARE_STATE_FILE), an allow-set served by a stub store (CHORUS_FUSEKI_QUERY),
 * dead pulse, temp side paths. Requests arrive "through the tunnel" (cf-ray) with
 * a public Host, so the loopback exemption does not apply.
 */
import fs from 'fs';
import os from 'os';
import path from 'path';
import http from 'http';
import crypto from 'crypto';
import type { AddressInfo } from 'net';

export const ALLOWED = 'https://pods.example/jeff/profile/card#me';
export const STRANGER = 'https://pods.example/stranger/profile/card#me';

// share-session.ts reads SHARE_STATE_FILE once, at import. Set it when THIS
// module loads, so import this helper before anything that imports
// share-session — or the guard key read is the live ~/.chorus/share-oidc.json.
const TMP = fs.mkdtempSync(path.join(os.tmpdir(), 'gate-'));
const KEY = crypto.randomBytes(32);
const b64u = (b: Buffer): string => b.toString('base64').replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
fs.writeFileSync(path.join(TMP, 'share-oidc.json'), JSON.stringify({ cookie_key: b64u(KEY) }));
process.env.SHARE_STATE_FILE = path.join(TMP, 'share-oidc.json');

export function useInProcessClearing() {
  const store = http.createServer((_req, res) => {
    res.setHeader('Content-Type', 'application/sparql-results+json');
    res.end(JSON.stringify({ results: { bindings: [{ webid: { value: ALLOWED } }] } }));
  });

  function guardCookie(webid: string, key: Buffer = KEY): string {
    const payload = b64u(Buffer.from(JSON.stringify({ webid, exp: Math.floor(Date.now() / 1000) + 600 })));
    return `chorus_share_session=${payload}.${b64u(crypto.createHmac('sha256', key).update(payload).digest())}`;
  }

  let base = '';
  let srv: { server: http.Server; io: { close: () => void } };

  beforeAll(async () => {
    await new Promise<void>((r) => store.listen(0, '127.0.0.1', () => r()));
    process.env.CHORUS_FUSEKI_QUERY = `http://127.0.0.1:${(store.address() as AddressInfo).port}/query`;
    process.env.PULSE_URL = 'http://127.0.0.1:1';
    process.env.CHORUS_ROOT = TMP;
    process.env.CLEARING_SCAN_DIR = TMP;
    process.env.CLEARING_PROJECTS_DIR = TMP;
    process.env.CLEARING_PULSE_FILE = path.join(TMP, 'pulse.json');
    srv = require('../../src/server');
    await new Promise<void>((r) => srv.server.listen(0, '127.0.0.1', () => r()));
    base = `http://127.0.0.1:${(srv.server.address() as AddressInfo).port}`;
  });

  afterAll(async () => {
    srv?.io.close();
    if (srv) await new Promise<void>((r) => srv.server.close(() => r()));
    await new Promise<void>((r) => store.close(() => r()));
    fs.rmSync(TMP, { recursive: true, force: true });
    delete process.env.CHORUS_SIGNIN_URL;
  });

  // A request from outside: through the tunnel (cf-ray), so loopback is not
  // "local", with the public Host the tunnel forwards. http.request, because
  // fetch refuses to set Host.
  function visit(method: string, p: string, cookie?: string): Promise<{ status: number; location: string | null; body: string }> {
    const body = method === 'POST' ? '{"from":"jeff","text":"hi"}' : '';
    return new Promise((resolve, reject) => {
      const req = http.request(`${base}${p}`, {
        method,
        headers: {
          'cf-ray': 'test', host: 'team.example.com',
          ...(cookie ? { cookie } : {}),
          ...(body ? { 'content-type': 'application/json', 'content-length': Buffer.byteLength(body) } : {}),
        },
      }, (res) => {
        let text = '';
        res.on('data', (c) => { text += c; });
        res.on('end', () => resolve({ status: res.statusCode || 0, location: (res.headers.location as string) || null, body: text }));
      });
      req.on('error', reject);
      req.end(body);
    });
  }

  return { visit, guardCookie, ALLOWED, STRANGER, base: () => base };
}
