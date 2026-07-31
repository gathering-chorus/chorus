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
  /** Where the JWKS is actually fetchable (env CHORUS_JWKS_URL — local CSS). */
  jwksUrl: string;
  /** SPARQL query against the pods dataset (security graph lives there). */
  sparql: (query: string) => Promise<unknown>;
  nowSecs: () => number;
  fetchFn?: typeof fetch;
  /** Cache TTL for JWKS + scope resolutions. Default 300s (the door cadence). */
  ttlSecs?: number;
}

interface SparqlBindings {
  results?: { bindings?: Array<Record<string, { value?: string }>> };
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

  async function keyFor(kid: string): Promise<crypto.KeyObject | null> {
    const now = deps.nowSecs();
    if (!jwks || jwks.expires <= now || !jwks.keys.has(kid)) {
      try {
        const res = await fetchFn(deps.jwksUrl);
        if (!res.ok) return jwks?.keys.get(kid) ?? null;
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
        jwks = { keys, expires: now + ttl };
      } catch {
        // unreachable JWKS: keep any still-cached keys until expiry, else fail
        return jwks && jwks.expires > now ? (jwks.keys.get(kid) ?? null) : null;
      }
    }
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
    const header = b64urlJson(parts[0]);
    if (!header) return { ok: false, reason: 'invalid' };
    if (header.alg === 'HS256') return { ok: false, reason: 'hs256-retired' };
    if (header.alg !== 'ES256') return { ok: false, reason: 'invalid' };
    const kid = typeof header.kid === 'string' ? header.kid : '';
    const key = kid ? await keyFor(kid) : null;
    if (!key) return { ok: false, reason: 'invalid' };

    const sigOk = crypto.verify(
      'sha256',
      Buffer.from(`${parts[0]}.${parts[1]}`),
      { key, dsaEncoding: 'ieee-p1363' },
      Buffer.from(parts[2], 'base64url'),
    );
    if (!sigOk) return { ok: false, reason: 'invalid' };

    const claims = b64urlJson(parts[1]);
    if (!claims) return { ok: false, reason: 'invalid' };
    if (typeof claims.iss !== 'string' || !sameIssuer(claims.iss, deps.issuer)) {
      return { ok: false, reason: 'invalid' };
    }
    if (typeof claims.exp !== 'number' || claims.exp <= deps.nowSecs()) {
      return { ok: false, reason: 'invalid' };
    }
    const webId = typeof claims.webid === 'string' ? claims.webid : '';
    if (!webId) return { ok: false, reason: 'invalid' };

    return { ok: true, webId, scope: await scopesFor(webId) };
  };
}
