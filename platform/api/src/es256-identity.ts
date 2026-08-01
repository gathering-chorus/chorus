/**
 * #3719 — ES256 identity verification + model-resolved scope for the security
 * envelope. Retires the HS256/shared-secret verify path (#3618/#3619) by
 * mirroring the owl-api door's contract (chorus-oidc, #3689):
 *
 *   - The token is a CSS-issued ES256 at+jwt: pure IDENTITY (iss, webid, exp).
 *     Verified against the CSS JWKS — one issuer, one key model, no secret.
 *   - What the identity may write is NOT a claim in the token. It is resolved
 *     from the model: chorus:hasScope edges on the Principal in
 *     urn:chorus:domains:security, cached on the same TTL cadence as the
 *     owl-api door's allow-set (300s). Grant/revoke = a model edit, live
 *     within one TTL.
 *   - An HS256 token is refused with a TYPED reason ('hs256-retired'), never
 *     silently accepted, never fallen back to.
 *
 * Fail-closed everywhere: unreachable JWKS, unknown kid, bad signature, wrong
 * issuer, expired, or an unanswerable scope query all end in refusal (empty
 * scope refuses every scoped surface). No cached failure is served past its
 * TTL window.
 */
import * as crypto from 'crypto';

const CHORUS_NS = 'https://jeffbridwell.com/chorus#';
const SECURITY_GRAPH = 'urn:chorus:domains:security';

export type VerifyResult =
  | { ok: true; webId: string; scope: string[] }
  | { ok: false; reason: 'hs256-retired' | 'invalid' };

export interface IdentityVerifierDeps {
  /** Logical CSS issuer the token's iss must equal (env CSS_ISSUER). */
  issuer: string;
  /** Where the JWKS is actually fetchable (env CHORUS_JWKS_URL — local CSS;
   *  a host differing from the issuer's gets the hairpin headers, #3720). */
  jwksUrl: string;
  /** SPARQL query against the pods dataset (security graph lives there). */
  sparql: (query: string) => Promise<unknown>;
  nowSecs: () => number;
  fetchFn?: typeof fetch;
  /** Cache TTL for JWKS + scope resolutions. Default 300s (the door cadence). */
  ttlSecs?: number;
}

interface SparqlBindings {
  // #3721 — the cell type is `| undefined` deliberately. SPARQL omits a
  // variable entirely from a row when it is unbound, so `b.s` really can be
  // absent; the old `Record<string, {value?: string}>` claimed otherwise and
  // made the `b.s?.value` guard below look redundant to the linter. The guard
  // was right and the type was wrong — fixed the type, kept the guard.
  results?: { bindings?: Array<Record<string, { value?: string } | undefined>> };
}

function b64urlJson(part: string): Record<string, unknown> | null {
  try {
    return JSON.parse(Buffer.from(part, 'base64url').toString('utf-8'));
  } catch {
    return null;
  }
}

/** Trailing-slash-insensitive issuer equality (CSS emits with, env may not). */
function sameIssuer(a: string, b: string): boolean {
  return a.replace(/\/+$/, '') === b.replace(/\/+$/, '');
}

/** #3720 — THE HAIRPIN. chorus-env-setup points CHORUS_JWKS_URL at the LOCAL
 * CSS (localhost:3001), which 500s a bare fetch: CSS only serves .oidc routes
 * as the logical issuer, so loopback callers must present the trusted-proxy
 * headers (Host + X-Forwarded-Proto/Host) — the same trick chorus-identity-
 * token and the #3669 Clearing exchange use. Without this, the door came up
 * refusing every valid token after the 2026-08-01 deploy and needed a
 * non-durable launchctl setenv to limp — dead on reboot. Headers are added
 * only when the JWKS host differs from the issuer host; a direct fetch to the
 * logical origin stays plain. */
function hairpinHeaders(jwksUrl: string, issuer: string): Record<string, string> {
  try {
    const jwksHost = new URL(jwksUrl).host;
    const iss = new URL(issuer);
    if (jwksHost === iss.host) return {};
    return { Host: iss.host, 'X-Forwarded-Proto': iss.protocol.replace(':', ''), 'X-Forwarded-Host': iss.host };
  } catch {
    return {};
  }
}

/** #3721 — extracted from keyFor. Fetches the JWKS and parses it into usable EC
 *  keys, distinguishing the two failure modes the cache policy treats
 *  differently (see keyFor): 'refused' = the endpoint answered non-2xx,
 *  'unreachable' = the fetch itself threw. A JWK that will not parse is skipped
 *  rather than failing the batch — one bad key must not blind the door to the
 *  rest. */
async function fetchJwks(
  fetchFn: typeof fetch,
  jwksUrl: string,
  issuer: string,
): Promise<Map<string, crypto.KeyObject> | 'refused' | 'unreachable'> {
  try {
    const res = await fetchFn(jwksUrl, { headers: hairpinHeaders(jwksUrl, issuer) });
    if (!res.ok) return 'refused';
    const body = (await res.json()) as { keys?: Array<Record<string, string>> };
    const keys = new Map<string, crypto.KeyObject>();
    for (const jwk of body.keys ?? []) {
      if (jwk.kty !== 'EC' || !jwk.kid) continue;
      try {
        keys.set(jwk.kid, crypto.createPublicKey({ key: jwk, format: 'jwk' }));
      } catch {
        /* skip unparseable keys; the rest may still verify */
      }
    }
    return keys;
  } catch {
    return 'unreachable';
  }
}

/** #3721 — extracted from verify. The header decides three outcomes, and the
 *  HS256 one must stay DISTINCT: #3618 retired the shared-secret path, and a
 *  caller has to be able to tell "you presented a retired algorithm" from
 *  "this token is malformed" — collapsing them would hide a live misconfigured
 *  client behind generic invalidity. */
function kidFromHeader(
  header: Record<string, unknown> | null,
): { kid: string } | { reason: 'hs256-retired' | 'invalid' } {
  if (!header) return { reason: 'invalid' };
  if (header.alg === 'HS256') return { reason: 'hs256-retired' };
  if (header.alg !== 'ES256') return { reason: 'invalid' };
  return { kid: typeof header.kid === 'string' ? header.kid : '' };
}

/** #3721 — extracted from verify. Every claim check is fail-closed and returns
 *  the same refusal, so they collapse to one predicate: the token names an
 *  issuer we trust, has not expired, and carries a WebID. */
function webIdFromClaims(
  claims: Record<string, unknown> | null,
  issuer: string,
  nowSecs: number,
): string | null {
  if (!claims) return null;
  if (typeof claims.iss !== 'string' || !sameIssuer(claims.iss, issuer)) return null;
  if (typeof claims.exp !== 'number' || claims.exp <= nowSecs) return null;
  return typeof claims.webid === 'string' && claims.webid ? claims.webid : null;
}

export function scopeQueryFor(webId: string): string {
  // The grant subject is the Principal INDIVIDUAL, joined to the token's
  // identity via its chorus:webId property (same join as the owl-api door's
  // PRINCIPAL_SCOPE_QUERY, #3689) — never the WebID IRI as subject.
  const lit = webId.replace(/\\/g, '\\\\').replace(/"/g, '\\"');
  return (
    `PREFIX chorus: <${CHORUS_NS}> ` +
    `SELECT ?s WHERE { GRAPH <${SECURITY_GRAPH}> { ?p a chorus:Principal ; ` +
    `chorus:webId "${lit}" ; chorus:hasScope ?s } }`
  );
}

export function createIdentityVerifier(deps: IdentityVerifierDeps): (token: string) => Promise<VerifyResult> {
  const fetchFn = deps.fetchFn ?? fetch;
  const ttl = deps.ttlSecs ?? 300;

  let jwks: { keys: Map<string, crypto.KeyObject>; expires: number } | null = null;
  const scopeCache = new Map<string, { scope: string[]; expires: number }>();

  // #3721 — keyFor was one function carrying fetch + parse + two distinct
  // failure policies, over both the complexity and cognitive-complexity limits.
  // Split into fetch-and-parse (below, module scope) and the cache policy here.
  // Behaviour is unchanged, including the deliberate asymmetry between the two
  // failure modes, which is easy to lose in a rewrite and is named explicitly:
  //   - JWKS answered non-2xx  → serve a cached key EVEN IF EXPIRED. The
  //     endpoint is up and told us no; the key we hold is the best evidence
  //     available and refusing every request is the worse failure.
  //   - JWKS unreachable/threw → serve a cached key ONLY while unexpired. We
  //     cannot distinguish "down" from "rotated away", so an expired key is
  //     not evidence and we fail closed.
  async function keyFor(kid: string): Promise<crypto.KeyObject | null> {
    const now = deps.nowSecs();
    const cached = jwks;
    const cachedIsFresh = cached !== null && cached.expires > now;
    if (cachedIsFresh && cached.keys.has(kid)) return cached.keys.get(kid) ?? null;

    const refreshed = await fetchJwks(fetchFn, deps.jwksUrl, deps.issuer);
    if (refreshed === 'unreachable') {
      // No `?.` on cached here: cachedIsFresh narrows it to non-null (TS
      // narrows through the const boolean alias), unlike the 'refused' branch
      // below where it can still be null.
      return cachedIsFresh ? (cached.keys.get(kid) ?? null) : null;
    }
    if (refreshed === 'refused') return cached?.keys.get(kid) ?? null;

    jwks = { keys: refreshed, expires: now + ttl };
    return jwks.keys.get(kid) ?? null;
  }

  async function scopesFor(webId: string): Promise<string[]> {
    const now = deps.nowSecs();
    const hit = scopeCache.get(webId);
    if (hit && hit.expires > now) return hit.scope;
    try {
      const res = (await deps.sparql(scopeQueryFor(webId))) as SparqlBindings;
      const scope = (res.results?.bindings ?? [])
        .map((b) => b.s?.value)
        .filter((v): v is string => typeof v === 'string' && v.length > 0);
      scopeCache.set(webId, { scope, expires: now + ttl });
      return scope;
    } catch {
      // fail closed: an unanswerable model is NO grants, and the failure is
      // not cached — the next request re-asks.
      return [];
    }
  }

  return async function verify(token: string): Promise<VerifyResult> {
    const parts = token.split('.');
    if (parts.length !== 3) return { ok: false, reason: 'invalid' };
    const head = kidFromHeader(b64urlJson(parts[0]));
    if ('reason' in head) return { ok: false, reason: head.reason };
    const key = head.kid ? await keyFor(head.kid) : null;
    if (!key) return { ok: false, reason: 'invalid' };

    const sigOk = crypto.verify(
      'sha256',
      Buffer.from(`${parts[0]}.${parts[1]}`),
      { key, dsaEncoding: 'ieee-p1363' },
      Buffer.from(parts[2], 'base64url'),
    );
    if (!sigOk) return { ok: false, reason: 'invalid' };

    const webId = webIdFromClaims(b64urlJson(parts[1]), deps.issuer, deps.nowSecs());
    if (!webId) return { ok: false, reason: 'invalid' };

    return { ok: true, webId, scope: await scopesFor(webId) };
  };
}
