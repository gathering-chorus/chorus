/**
 * #3619 — service auth for integration tests that hit the LIVE chorus-api.
 *
 * Every mutation endpoint is behind the security envelope; these suites are
 * real consumers (the nightly 03:0x write bursts in Loki were exactly them),
 * so they carry credentials like any other caller — deploy-before-require.
 *
 * withServiceAuth() wraps global fetch for the suite: any mutating request
 * (POST/PUT/DELETE/PATCH) gets a scoped Bearer minted from the local realm
 * secret. Fail-open: no secret → requests go bare and the envelope decides
 * (suites then fail visibly with 401s, naming exactly what's missing).
 */
import * as crypto from 'crypto';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';

const SCOPES = [
  'urn:chorus:ops',
  'urn:chorus:index',
  'urn:chorus:icd',
  'urn:chorus:cards',
  'urn:chorus:domains:code',
];

function resolveSecret(): string | null {
  if (process.env.CHORUS_SERVICE_TOKEN_SECRET) return process.env.CHORUS_SERVICE_TOKEN_SECRET;
  try {
    const raw = fs.readFileSync(
      process.env.CHORUS_REALM_ENV_PATH ||
        path.join(os.homedir(), '.chorus', 'secrets', 'chorus-realm.env'),
      'utf8'
    );
    for (const line of raw.split('\n')) {
      const m = /^(?:export\s+)?CHORUS_SERVICE_TOKEN_SECRET=(.+)$/.exec(line.trim());
      if (m) return m[1].replace(/^["']|["']$/g, '');
    }
  } catch { /* fail open */ }
  return null;
}

export function mintTestToken(): string | null {
  const secret = resolveSecret();
  if (!secret) return null;
  const b64 = (s: Buffer | string) => Buffer.from(s).toString('base64url');
  const header = b64(JSON.stringify({ alg: 'HS256', typ: 'JWT' }));
  const claims = b64(JSON.stringify({
    webId: 'http://localhost:3000/pods/chorus/_agents/chorus-sdk/profile/card.ttl#me',
    aud: 'chorus',
    exp: Math.floor(Date.now() / 1000) + 3600,
    scope: SCOPES,
  }));
  const sig = b64(crypto.createHmac('sha256', secret).update(`${header}.${claims}`).digest());
  return `${header}.${claims}.${sig}`;
}

const WRITE = new Set(['POST', 'PUT', 'DELETE', 'PATCH']);

/** Wrap global fetch for the suite: mutating requests carry the test token. */
export function withServiceAuth(): void {
  const token = mintTestToken();
  if (!token) return; // fail open — envelope will 401 and the suite says why
  const real = globalThis.fetch;
  globalThis.fetch = ((input: RequestInfo | URL, init?: RequestInit) => {
    const method = (init?.method || 'GET').toUpperCase();
    if (WRITE.has(method)) {
      init = { ...init, headers: { ...(init?.headers as Record<string, string>), Authorization: `Bearer ${token}` } };
    }
    return real(input as never, init);
  }) as typeof fetch;
}
