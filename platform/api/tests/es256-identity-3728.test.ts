// @test-type: unit
/**
 * #3728 — es256-identity.ts to Rust (chorus-oidc) parity. #3719 introduced the
 * TS verifier with NO dedicated test while the Rust door has 30+; this brings
 * the TS door to the same contract and PINS the two things #3728 fixes:
 *
 *   1. ONE scope query, from ONE source. The verifier no longer hand-writes a
 *      per-webId SELECT; it runs the canonical BULK query (the same
 *      urn:chorus:domains:security / chorus:hasScope resolver the Rust door
 *      uses, PRINCIPAL_SCOPE_QUERY) injected as deps.scopeQuery, and groups
 *      webId→scopes exactly like Rust's resolve_principal_scopes. The canonical
 *      text lives in src/sparql/principal-scope.rq — this test asserts the file
 *      exists and carries the security-graph/hasScope shape both doors share.
 *
 *   2. The JWKS-cache asymmetry (#3719's flagged bug) is REMOVED. A non-2xx
 *      JWKS response must NOT serve a cached key past its TTL — same fail-closed
 *      posture as an unreachable fetch, matching Rust (both collapse to
 *      JwksUnreachable). The negative-proof case below FAILS against the old
 *      "serve expired on non-2xx" code and passes only once it is symmetric.
 *
 * This test brings its own world (#3528): a generated EC keypair standing in
 * for CSS, a stub fetch serving its JWKS, a stub sparql serving the bulk
 * chorus:hasScope rows. No live stack, no $HOME, no shared secret.
 */
import * as crypto from 'crypto';
import * as fs from 'fs';
import * as path from 'path';
import { createIdentityVerifier } from '../src/es256-identity';

const NOW = 1_800_000_000; // fixed clock (secs)
const TTL = 300;
const ISSUER = 'https://id.test.example/';
const KID = 'test-css-key-1';
const WEBID_A = 'http://localhost:3000/pods/chorus/_agents/reindex-worker/profile/card.ttl#me';
const WEBID_B = 'http://localhost:3000/pods/chorus/_agents/silas/profile/card.ttl#me';
const WEBID_UNGRANTED = 'http://localhost:3000/pods/chorus/_agents/nobody/profile/card.ttl#me';
const SCOPE_A1 = 'urn:chorus:index';
const SCOPE_A2 = 'urn:chorus:domains:code';
const SCOPE_B = 'urn:chorus:domains:security';

// The canonical bulk query, read from the ONE source both doors bind to.
const CANONICAL_RQ = path.resolve(__dirname, '..', 'src', 'sparql', 'principal-scope.rq');
const BULK_QUERY = fs.readFileSync(CANONICAL_RQ, 'utf-8').trim();

const { publicKey, privateKey } = crypto.generateKeyPairSync('ec', { namedCurve: 'prime256v1' });
const JWKS = { keys: [{ ...publicKey.export({ format: 'jwk' }), kid: KID, alg: 'ES256', use: 'sig' }] };

function b64url(buf: Buffer | string): string {
  return Buffer.from(buf).toString('base64url');
}

function mintEs256(opts: { webid?: string; expAt?: number; iss?: string; kid?: string; alg?: string } = {}): string {
  const header = b64url(JSON.stringify({ alg: opts.alg ?? 'ES256', typ: 'at+jwt', kid: opts.kid ?? KID }));
  const claims = b64url(JSON.stringify({
    iss: opts.iss ?? ISSUER,
    webid: opts.webid ?? WEBID_A,
    exp: opts.expAt ?? NOW + TTL,
    // NO scope claim — identity only. Scope is model data (#3689).
  }));
  const sig = crypto.sign('sha256', Buffer.from(`${header}.${claims}`), { key: privateKey, dsaEncoding: 'ieee-p1363' });
  return `${header}.${claims}.${b64url(sig)}`;
}

function mintHs256(): string {
  const header = b64url(JSON.stringify({ alg: 'HS256', typ: 'JWT' }));
  const claims = b64url(JSON.stringify({ webId: WEBID_A, exp: NOW + TTL, scope: [SCOPE_A1] }));
  const sig = b64url(crypto.createHmac('sha256', 'the-retired-secret').update(`${header}.${claims}`).digest());
  return `${header}.${claims}.${sig}`;
}

function mintAlgNone(): string {
  const header = b64url(JSON.stringify({ alg: 'none', typ: 'JWT' }));
  const claims = b64url(JSON.stringify({ iss: ISSUER, webid: WEBID_A, exp: NOW + TTL }));
  return `${header}.${claims}.`;
}

// Bulk model: A granted two scopes, B granted one, nobody has nothing. One row
// per (webid, scope) edge — the shape resolve_principal_scopes parses.
function stubSparql(query: string): Promise<unknown> {
  if (query.trim() !== BULK_QUERY) return Promise.resolve({ results: { bindings: [] } });
  return Promise.resolve({
    results: {
      bindings: [
        { v: { value: `${WEBID_A} ${SCOPE_A1}` } },
        { v: { value: `${WEBID_A} ${SCOPE_A2}` } },
        { v: { value: `${WEBID_B} ${SCOPE_B}` } },
      ],
    },
  });
}

const okFetch = (async () => ({ ok: true, json: async () => JWKS })) as unknown as typeof fetch;

function makeVerify(overrides: {
  sparql?: (q: string) => Promise<unknown>;
  fetchFn?: typeof fetch;
  nowSecs?: () => number;
} = {}) {
  return createIdentityVerifier({
    issuer: ISSUER,
    jwksUrl: 'http://css.test/.oidc/jwks',
    scopeQuery: BULK_QUERY,
    sparql: overrides.sparql ?? stubSparql,
    nowSecs: overrides.nowSecs ?? (() => NOW),
    fetchFn: overrides.fetchFn ?? okFetch,
    ttlSecs: TTL,
  });
}

describe('#3728 — es256-identity parity with the Rust door', () => {
  test('the canonical scope query is ONE source: the .rq file carries the shared security-graph/hasScope shape', () => {
    expect(fs.existsSync(CANONICAL_RQ)).toBe(true);
    expect(BULK_QUERY).toContain('urn:chorus:domains:security');
    expect(BULK_QUERY).toContain('chorus:hasScope');
    expect(BULK_QUERY).toContain('chorus:Principal');
  });

  test('valid ES256 identity → ok, scope resolved from the model (grouped, bulk)', async () => {
    const r = await makeVerify()(mintEs256({ webid: WEBID_A }));
    expect(r).toEqual({ ok: true, webId: WEBID_A, scope: [SCOPE_A1, SCOPE_A2] });
  });

  test('scope does not leak between principals (B gets only B\'s grant)', async () => {
    const r = await makeVerify()(mintEs256({ webid: WEBID_B }));
    expect(r).toEqual({ ok: true, webId: WEBID_B, scope: [SCOPE_B] });
  });

  test('valid identity, no grant in model → ok identity, empty scope (every scoped write refuses)', async () => {
    const r = await makeVerify()(mintEs256({ webid: WEBID_UNGRANTED }));
    expect(r).toEqual({ ok: true, webId: WEBID_UNGRANTED, scope: [] });
  });

  test('HS256 token → typed hs256-retired refusal (never silently accepted)', async () => {
    const r = await makeVerify()(mintHs256());
    expect(r).toEqual({ ok: false, reason: 'hs256-retired' });
  });

  test('alg:none → invalid', async () => {
    const r = await makeVerify()(mintAlgNone());
    expect(r).toEqual({ ok: false, reason: 'invalid' });
  });

  test('non-ES256 alg (RS256) → invalid', async () => {
    const r = await makeVerify()(mintEs256({ alg: 'RS256' }));
    expect(r).toEqual({ ok: false, reason: 'invalid' });
  });

  test('expired token → invalid', async () => {
    const r = await makeVerify()(mintEs256({ expAt: NOW - 1 }));
    expect(r).toEqual({ ok: false, reason: 'invalid' });
  });

  test('wrong issuer → invalid (validly signed by SOMEONE is not enough)', async () => {
    const r = await makeVerify()(mintEs256({ iss: 'https://evil.example/' }));
    expect(r).toEqual({ ok: false, reason: 'invalid' });
  });

  test('tampered signature → invalid', async () => {
    const tok = mintEs256({ webid: WEBID_A });
    const parts = tok.split('.');
    // flip the FIRST sig char — it encodes real r‖s bytes (the last char is only
    // padding bits and decodes identically, which would not tamper anything).
    const sig = (parts[2][0] === 'A' ? 'B' : 'A') + parts[2].slice(1);
    const r = await makeVerify()(`${parts[0]}.${parts[1]}.${sig}`);
    expect(r).toEqual({ ok: false, reason: 'invalid' });
  });

  test('unknown kid (key CSS never published) → invalid', async () => {
    const r = await makeVerify()(mintEs256({ kid: 'not-in-jwks' }));
    expect(r).toEqual({ ok: false, reason: 'invalid' });
  });

  test('model unanswerable (sparql throws) → identity ok, scope fail-closed to empty, not cached', async () => {
    const r = await makeVerify({ sparql: () => Promise.reject(new Error('fuseki down')) })(mintEs256({ webid: WEBID_A }));
    expect(r).toEqual({ ok: true, webId: WEBID_A, scope: [] });
  });

  test('malformed token (not three parts) → invalid', async () => {
    const r = await makeVerify()('not.a.valid.jwt.shape');
    expect(r).toEqual({ ok: false, reason: 'invalid' });
  });

  // ── the #3728 headline: the cache-asymmetry fix, as a NEGATIVE PROOF ──
  test('non-2xx JWKS past TTL does NOT serve an expired key (the #3719 asymmetry, fixed)', async () => {
    let clock = NOW;
    let mode: 'ok' | 'refused' = 'ok';
    const flakyFetch = (async () => {
      if (mode === 'refused') return { ok: false, status: 503, json: async () => ({}) };
      return { ok: true, json: async () => JWKS };
    }) as unknown as typeof fetch;

    const verify = makeVerify({ fetchFn: flakyFetch, nowSecs: () => clock });

    // t0: warm the cache with a good fetch
    expect((await verify(mintEs256({ webid: WEBID_A }))).ok).toBe(true);

    // key is now expired, and CSS answers non-2xx
    clock = NOW + TTL + 1;
    mode = 'refused';

    // OLD behaviour served the expired key here → ok:true. Parity says fail closed.
    const r = await verify(mintEs256({ webid: WEBID_A, expAt: clock + TTL }));
    expect(r).toEqual({ ok: false, reason: 'invalid' });
  });

  test('unreachable JWKS past TTL also fails closed (symmetry with non-2xx)', async () => {
    let clock = NOW;
    let mode: 'ok' | 'throw' = 'ok';
    const flakyFetch = (async () => {
      if (mode === 'throw') throw new Error('ECONNREFUSED');
      return { ok: true, json: async () => JWKS };
    }) as unknown as typeof fetch;

    const verify = makeVerify({ fetchFn: flakyFetch, nowSecs: () => clock });
    expect((await verify(mintEs256({ webid: WEBID_A }))).ok).toBe(true);

    clock = NOW + TTL + 1;
    mode = 'throw';
    const r = await verify(mintEs256({ webid: WEBID_A, expAt: clock + TTL }));
    expect(r).toEqual({ ok: false, reason: 'invalid' });
  });

  test('a fresh cached key within TTL verifies without refetching', async () => {
    let calls = 0;
    const countingFetch = (async () => {
      calls += 1;
      return { ok: true, json: async () => JWKS };
    }) as unknown as typeof fetch;

    const verify = makeVerify({ fetchFn: countingFetch });
    await verify(mintEs256({ webid: WEBID_A }));
    await verify(mintEs256({ webid: WEBID_B }));
    expect(calls).toBe(1); // second verify hit the warm JWKS cache
  });
});
