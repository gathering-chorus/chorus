// @domain: awareness
// @test-type: unit — callerRole and roleFromWebId in caller-identity.ts against test ES256 keys and an injected JWKS fetch; no live services
// @card: #4965
// @owner: kade
/**
 * #4965 — every way a bearer token fails to prove a role reads null (fail
 * closed): a malformed token, a non-ES256 header, an unknown kid, a JWKS that
 * answers an error or nothing, and stale keys once the JWKS is gone. A good
 * token still proves its role, and the JWKS fetch carries the issuer's host
 * only when it is dialled at another address.
 */
import * as crypto from 'crypto';
import { callerRole, configureCallerIdentity, roleFromWebId } from './caller-identity';

const ISSUER = 'https://id.example.test/';
const KID = 'cov-kid';
const { privateKey, publicKey } = crypto.generateKeyPairSync('ec', { namedCurve: 'P-256' });
const jwk = { ...publicKey.export({ format: 'jwk' }), kid: KID };
const b64 = (o: object) => Buffer.from(JSON.stringify(o)).toString('base64url');

function token(claims: Record<string, unknown>, head: Record<string, unknown> = { alg: 'ES256', kid: KID }): string {
  const h = b64(head);
  const b = b64({ iss: ISSUER, webid: 'https://id.example.test/kade/profile/card#me', ...claims });
  const sig = crypto.sign('sha256', Buffer.from(`${h}.${b}`), { key: privateKey, dsaEncoding: 'ieee-p1363' });
  return `${h}.${b}.${sig.toString('base64url')}`;
}

type Seen = { url: string; headers: Record<string, string> };
function jwksFetch(seen: Seen[], answer: () => Response | Promise<Response>): typeof fetch {
  return (async (url: string, init?: { headers?: Record<string, string> }) => {
    seen.push({ url, headers: init?.headers ?? {} });
    return answer();
  }) as unknown as typeof fetch;
}
const ok = (keys: unknown[]) => () => new Response(JSON.stringify({ keys }), { status: 200 });
const farFuture = Math.floor(Date.now() / 1000) + 3600;

afterAll(() => configureCallerIdentity());

describe('#4965 a good token proves its role', () => {
  test('on the real clock, an unexpired token names kade; the loopback JWKS fetch carries the issuer host', async () => {
    const seen: Seen[] = [];
    configureCallerIdentity({ issuer: ISSUER, jwksUrl: 'http://127.0.0.1:3001/.oidc/jwks', fetchFn: jwksFetch(seen, ok([{ kty: 'RSA', kid: 'rsa-one' }, jwk])) });
    expect(await callerRole(`Bearer ${token({ exp: farFuture })}`)).toBe('kade');
    expect(seen[0].headers).toEqual({ Host: 'id.example.test', 'X-Forwarded-Proto': 'https', 'X-Forwarded-Host': 'id.example.test' });
  });

  test('a JWKS dialled at the issuer host itself gets no proxy headers', async () => {
    const seen: Seen[] = [];
    configureCallerIdentity({ issuer: ISSUER, jwksUrl: 'https://id.example.test/.oidc/jwks', fetchFn: jwksFetch(seen, ok([jwk])) });
    expect(await callerRole(`Bearer ${token({ exp: farFuture })}`)).toBe('kade');
    expect(seen[0].headers).toEqual({});
  });

  test('an issuer that is not a URL sends no proxy headers and proves nothing', async () => {
    const seen: Seen[] = [];
    configureCallerIdentity({ issuer: 'not a url', jwksUrl: 'http://127.0.0.1:3001/.oidc/jwks', fetchFn: jwksFetch(seen, ok([jwk])) });
    expect(await callerRole(`Bearer ${token({ exp: farFuture, iss: 'not a url' })}`)).toBeNull();
    expect(seen[0].headers).toEqual({});
  });
});

describe('#4965 negative: every unproven token reads null', () => {
  beforeEach(() => configureCallerIdentity({ issuer: ISSUER, jwksUrl: 'http://127.0.0.1:1/jwks', fetchFn: jwksFetch([], ok([jwk])) }));

  test('a token without three parts', async () => {
    expect(await callerRole('Bearer only.two')).toBeNull();
  });

  test('a header that is not base64 JSON', async () => {
    const t = token({ exp: farFuture });
    expect(await callerRole(`Bearer !!!${t.slice(t.indexOf('.'))}`)).toBeNull();
  });

  test('a header that is not ES256', async () => {
    expect(await callerRole(`Bearer ${token({ exp: farFuture }, { alg: 'HS256', kid: KID })}`)).toBeNull();
  });

  test('a kid the JWKS does not have', async () => {
    expect(await callerRole(`Bearer ${token({ exp: farFuture }, { alg: 'ES256', kid: 'other-kid' })}`)).toBeNull();
  });

  test('a JWKS that answers HTTP 500', async () => {
    configureCallerIdentity({ issuer: ISSUER, jwksUrl: 'http://127.0.0.1:1/jwks', fetchFn: jwksFetch([], () => new Response('', { status: 500 })) });
    expect(await callerRole(`Bearer ${token({ exp: farFuture })}`)).toBeNull();
  });

  test('a JWKS fetch that throws', async () => {
    configureCallerIdentity({ issuer: ISSUER, jwksUrl: 'http://127.0.0.1:1/jwks', fetchFn: jwksFetch([], () => { throw new Error('ECONNREFUSED'); }) });
    expect(await callerRole(`Bearer ${token({ exp: farFuture })}`)).toBeNull();
  });

  test('keys that went stale while the JWKS is gone stop proving the role', async () => {
    let now = 2_000_000_000;
    let up = true;
    configureCallerIdentity({
      issuer: ISSUER, jwksUrl: 'http://127.0.0.1:1/jwks', nowSecs: () => now, ttlSecs: 300,
      fetchFn: jwksFetch([], () => { if (!up) throw new Error('down'); return ok([jwk])(); }),
    });
    const t = `Bearer ${token({ exp: now + 10_000 })}`;
    expect(await callerRole(t)).toBe('kade');
    up = false;
    now += 100; // still fresh: the cached key proves it with the JWKS down
    expect(await callerRole(t)).toBe('kade');
    now += 1000; // stale: fail closed
    expect(await callerRole(t)).toBeNull();
  });

  test('a WebID that is not a URL names no role', () => {
    expect(roleFromWebId('not a url', ISSUER)).toBeNull();
    expect(roleFromWebId('https://id.example.test/kade/profile/card#me', ISSUER)).toBe('kade');
  });
});
