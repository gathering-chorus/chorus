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
  // The stub store answers the three questions the Clearing asks the security
  // graph: who is allowed (webids), who is who (principal ↔ webid), and who is
  // a person (principalKind "person"): jeff and marknakib here.
  const store = http.createServer((req, res) => {
    // #4432 — the same stub answers chorus-api's role rows, so the tiles come
    // from here and never from the live :3340 (they used to, silently).
    if ((req.url || '').startsWith('/api/chorus/context/roles')) {
      res.setHeader('Content-Type', 'application/json');
      res.end(JSON.stringify({ data: { roles: ['kade', 'silas', 'wren'].map((role) => ({ role, name: role, state: 'idle', stale: true, lastActivity: null })) } }));
      return;
    }
    if ((req.url || '').startsWith('/api/')) { res.statusCode = 404; res.end('{}'); return; }
    const q = decodeURIComponent((req.url || '').split('query=')[1] || '');
    res.setHeader('Content-Type', 'application/sparql-results+json');
    const P = 'https://jeffbridwell.com/chorus#';
    if (q.includes('principalKind')) {
      res.end(JSON.stringify({ results: { bindings: [
        { p: { value: `${P}principal-jeff` }, label: { value: 'Jeff Bridwell' }, webid: { value: 'https://id.example/jeff/profile/card#me' }, host: { value: 'jeffbridwell' } },
        { p: { value: `${P}principal-marknakib` }, label: { value: 'Mark Nakib' } },
      ] } }));
    } else if (q.includes('?p ?webid')) {
      res.end(JSON.stringify({ results: { bindings: [{ p: { value: `${P}principal-jeff` }, webid: { value: ALLOWED } }] } }));
    } else {
      res.end(JSON.stringify({ results: { bindings: [{ webid: { value: ALLOWED } }] } }));
    }
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
    process.env.CHORUS_API_BASE = `http://127.0.0.1:${(store.address() as AddressInfo).port}`;
    process.env.CHORUS_ROOT = TMP;
    process.env.CLEARING_SCAN_DIR = TMP;
    process.env.CLEARING_PROJECTS_DIR = TMP;
    process.env.CLEARING_PULSE_FILE = path.join(TMP, 'pulse.json');
    srv = require('../../src/server');
    await new Promise<void>((r) => srv.server.listen(0, '127.0.0.1', () => r()));
    base = `http://127.0.0.1:${(srv.server.address() as AddressInfo).port}`;
    // #4432 — the role tiles arrive with the poller's first API answer.
    for (let i = 0; i < 40; i++) {
      const tiles = await (await fetch(`${base}/api/tiles`)).json() as unknown[];
      if (tiles.length > 1) break;
      await new Promise((r) => setTimeout(r, 50));
    }
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

  return { visit, guardCookie, ALLOWED, STRANGER, base: () => base, dir: TMP };
}
