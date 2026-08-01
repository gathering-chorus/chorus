// @test-type: unit
/**
 * #3618/#3719 — the security envelope (hermetic tier).
 *
 * Core under test: decideEnvelope(request, deps) — the pure decision function
 * behind the Express adapter — composed with createIdentityVerifier. This test
 * brings its own world (#3528): a generated EC keypair standing in for CSS, a
 * stub fetch serving its JWKS, a stub sparql serving chorus:hasScope rows.
 * No live stack, no $HOME, no shared secret (there is none any more).
 *
 * Contract (mirrors the owl-api door, #3689):
 *   - request matches a secured surface + no/invalid Bearer      → refuse 401
 *   - HS256 token                                                → refuse 401 hs256-retired (typed)
 *   - valid ES256 identity, Principal not granted the scope      → refuse 403
 *   - valid ES256 identity + model grant                         → pass
 *   - scope comes from the MODEL, never from a token claim
 *   - request matches no secured surface                         → pass, no events
 */
import * as crypto from 'crypto';
import {
  decideEnvelope,
  type EnvelopeRequest,
  type EnvelopeDeps,
  type SecuredSurface,
} from '../src/security-envelope';
import { createIdentityVerifier, scopeQueryFor } from '../src/es256-identity';

const NOW = 1_800_000_000; // fixed clock (secs)
const ISSUER = 'https://id.test.example/';
const KID = 'test-css-key-1';
const GRANTED_WEBID = 'http://localhost:3000/pods/chorus/_agents/reindex-worker/profile/card.ttl#me';
const NOBODY_WEBID = 'http://localhost:3000/pods/chorus/_agents/nobody/profile/card.ttl#me';

const { publicKey, privateKey } = crypto.generateKeyPairSync('ec', { namedCurve: 'prime256v1' });
const JWKS = { keys: [{ ...publicKey.export({ format: 'jwk' }), kid: KID, alg: 'ES256', use: 'sig' }] };

function b64url(buf: Buffer | string): string {
  return Buffer.from(buf).toString('base64url');
}

function mintEs256(opts: { webid?: string; expAt?: number; iss?: string; kid?: string } = {}): string {
  const header = b64url(JSON.stringify({ alg: 'ES256', typ: 'at+jwt', kid: opts.kid ?? KID }));
  const claims = b64url(JSON.stringify({
    iss: opts.iss ?? ISSUER,
    webid: opts.webid ?? GRANTED_WEBID,
    exp: opts.expAt ?? NOW + 300,
    // NO scope claim — identity only. Scope is model data (#3689).
  }));
  const sig = crypto.sign('sha256', Buffer.from(`${header}.${claims}`), { key: privateKey, dsaEncoding: 'ieee-p1363' });
  return `${header}.${claims}.${b64url(sig)}`;
}

function mintHs256(): string {
  const header = b64url(JSON.stringify({ alg: 'HS256', typ: 'JWT' }));
  const claims = b64url(JSON.stringify({ webId: GRANTED_WEBID, aud: 'chorus', exp: NOW + 300, scope: ['urn:chorus:index'] }));
  const sig = b64url(crypto.createHmac('sha256', 'the-retired-secret').update(`${header}.${claims}`).digest());
  return `${header}.${claims}.${sig}`;
}

const SURFACES: SecuredSurface[] = [
  { method: 'POST', pathPrefix: '/api/chorus/reindex', requiresScope: 'urn:chorus:index', surface: 'surface-index-writes' },
  { method: 'POST', pathPrefix: '/api/athena/discover-', requiresScope: 'urn:chorus:domains:code', surface: 'surface-discover-writes' },
];

// stub model: reindex-worker is granted urn:chorus:index; nobody has nothing
function stubSparql(query: string): Promise<unknown> {
  const rows = query === scopeQueryFor(GRANTED_WEBID) ? [{ s: { value: 'urn:chorus:index' } }] : [];
  return Promise.resolve({ results: { bindings: rows } });
}

const stubFetch = (async () => ({ ok: true, json: async () => JWKS })) as unknown as typeof fetch;

// #3720 — hairpin pin: when the JWKS host differs from the issuer host, the
// fetch must carry Host + X-Forwarded-* naming the ISSUER (CSS refuses bare
// loopback .oidc fetches); same-host fetches stay plain.
test('JWKS fetch hairpins when jwks host differs from issuer host (#3720)', async () => {
  const seen: Array<Record<string, string>> = [];
  const spyFetch = (async (_url: unknown, init?: { headers?: Record<string, string> }) => {
    seen.push(init?.headers ?? {});
    return { ok: true, json: async () => JWKS };
  }) as unknown as typeof fetch;
  const v = createIdentityVerifier({
    issuer: ISSUER, jwksUrl: 'http://localhost:3001/.oidc/jwks',
    sparql: stubSparql, nowSecs: () => NOW, fetchFn: spyFetch,
  });
  await v(mintEs256());
  expect(seen[0]).toMatchObject({
    Host: 'id.test.example',
    'X-Forwarded-Proto': 'https',
    'X-Forwarded-Host': 'id.test.example',
  });

  const seenPlain: Array<Record<string, string>> = [];
  const plainFetch = (async (_url: unknown, init?: { headers?: Record<string, string> }) => {
    seenPlain.push(init?.headers ?? {});
    return { ok: true, json: async () => JWKS };
  }) as unknown as typeof fetch;
  const v2 = createIdentityVerifier({
    issuer: ISSUER, jwksUrl: 'https://id.test.example/.oidc/jwks',
    sparql: stubSparql, nowSecs: () => NOW, fetchFn: plainFetch,
  });
  await v2(mintEs256());
  expect(seenPlain[0]).toEqual({});
});

function makeVerify(overrides: { sparql?: (q: string) => Promise<unknown> } = {}) {
  return createIdentityVerifier({
    issuer: ISSUER,
    jwksUrl: 'http://css.test/.oidc/jwks',
    sparql: overrides.sparql ?? stubSparql,
    nowSecs: () => NOW,
    fetchFn: stubFetch,
  });
}

function deps(overrides: Partial<EnvelopeDeps> = {}): EnvelopeDeps {
  return { surfaces: SURFACES, verify: makeVerify(), ...overrides };
}

function req(overrides: Partial<EnvelopeRequest> = {}): EnvelopeRequest {
  return { method: 'POST', path: '/api/chorus/reindex', authorization: '', ...overrides };
}

describe('decideEnvelope (#3618/#3719)', () => {
  test('secured surface without Bearer → refuse 401 authn-missing + attempt/refused events', async () => {
    const d = await decideEnvelope(req(), deps());
    expect(d.action).toBe('refuse');
    expect(d.status).toBe(401);
    expect(d.body?.error).toBe('authn-missing');
    expect(d.events.map(e => e.event)).toEqual(
      ['security.envelope.attempt', 'security.envelope.refused']);
    expect(d.events[1].fields.surface).toBe('surface-index-writes');
  });

  test('valid ES256 identity + model grant → pass + allowed event carries webId', async () => {
    const d = await decideEnvelope(req({ authorization: `Bearer ${mintEs256()}` }), deps());
    expect(d.action).toBe('pass');
    const allowed = d.events.find(e => e.event === 'security.envelope.allowed');
    expect(allowed?.fields.webId).toContain('reindex-worker');
  });

  test('scope comes from the MODEL: grant does not cover the discover surface → 403', async () => {
    const d = await decideEnvelope(
      req({ path: '/api/athena/discover-code', authorization: `Bearer ${mintEs256()}` }),
      deps());
    expect(d.action).toBe('refuse');
    expect(d.status).toBe(403);
    expect(d.body?.error).toBe('out-of-scope');
  });

  test('grantless Principal → refuse 403 (absence of hasScope is absence of authorization)', async () => {
    const d = await decideEnvelope(
      req({ authorization: `Bearer ${mintEs256({ webid: NOBODY_WEBID })}` }), deps());
    expect(d.action).toBe('refuse');
    expect(d.status).toBe(403);
  });

  test('HS256 token → refuse 401 with the TYPED hs256-retired reason, never verified', async () => {
    const d = await decideEnvelope(req({ authorization: `Bearer ${mintHs256()}` }), deps());
    expect(d.action).toBe('refuse');
    expect(d.status).toBe(401);
    expect(d.body?.error).toBe('hs256-retired');
    expect(d.events[1].fields.reason).toBe('hs256-retired');
  });

  test('expired token → refuse 401', async () => {
    const d = await decideEnvelope(
      req({ authorization: `Bearer ${mintEs256({ expAt: NOW - 10 })}` }), deps());
    expect(d.action).toBe('refuse');
    expect(d.status).toBe(401);
  });

  test('forged signature → refuse 401', async () => {
    const good = mintEs256();
    const forged = good.slice(0, good.lastIndexOf('.') + 1) + b64url('not-a-real-sig');
    const d = await decideEnvelope(req({ authorization: `Bearer ${forged}` }), deps());
    expect(d.action).toBe('refuse');
    expect(d.status).toBe(401);
  });

  test('wrong issuer → refuse 401', async () => {
    const d = await decideEnvelope(
      req({ authorization: `Bearer ${mintEs256({ iss: 'https://evil.example/' })}` }), deps());
    expect(d.action).toBe('refuse');
    expect(d.status).toBe(401);
  });

  test('unknown kid → refuse 401 (no key, no verify)', async () => {
    const d = await decideEnvelope(
      req({ authorization: `Bearer ${mintEs256({ kid: 'not-in-jwks' })}` }), deps());
    expect(d.action).toBe('refuse');
    expect(d.status).toBe(401);
  });

  test('unanswerable scope query fails CLOSED → 403, not a pass', async () => {
    const d = await decideEnvelope(
      req({ authorization: `Bearer ${mintEs256()}` }),
      deps({ verify: makeVerify({ sparql: () => Promise.reject(new Error('fuseki down')) }) }));
    expect(d.action).toBe('refuse');
    expect(d.status).toBe(403);
  });

  test('prefix match secures the whole discover class', async () => {
    const d = await decideEnvelope(req({ path: '/api/athena/discover-code' }), deps());
    expect(d.action).toBe('refuse');
    expect(d.status).toBe(401);
    expect(d.events[0].fields.surface).toBe('surface-discover-writes');
  });

  test('unsecured surface passes untouched with zero events — mixed-state by construction', async () => {
    const d = await decideEnvelope(req({ path: '/api/cards/add' }), deps());
    expect(d.action).toBe('pass');
    expect(d.events).toHaveLength(0);
  });

  test('GET on a secured path prefix is not gated (mutation-only envelope, reads stay open)', async () => {
    const d = await decideEnvelope(req({ method: 'GET' }), deps());
    expect(d.action).toBe('pass');
    expect(d.events).toHaveLength(0);
  });

  test('empty surface table gates nothing (pre-generation boot safety)', async () => {
    const d = await decideEnvelope(req(), deps({ surfaces: [] }));
    expect(d.action).toBe('pass');
    expect(d.events).toHaveLength(0);
  });
});
