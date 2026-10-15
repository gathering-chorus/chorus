// #4534 — a nudge sender proves who it is with its own identity token.
//
// The shared secret (#3485) lives in a 0600 file under jeffbridwell's HOME.
// Abby runs as her own macOS user (chorus-abby-normal), so nudge-send under her
// account never finds the file and pulse refused every nudge she sent
// (403 not-canonical-caller, 2026-10-14 19:34 onward). The runner already hands
// every tool run the caller's ES256 identity token (CHORUS_IDENTITY_TOKEN), so
// pulse checks that instead of sharing the secret with a second account.
//
// The check is the same one the chorus-api envelope makes (es256-identity.ts,
// #3719): CSS-issued ES256, verified against the CSS JWKS, issuer and expiry
// checked, fail closed everywhere. Pulse needs identity only, not scope: the
// token's WebID names the role, and that role must be the nudge's `from`. A
// valid token for one role cannot send as another.

import * as crypto from 'crypto';

export interface CallerIdentityDeps {
  issuer: string;
  jwksUrl: string;
  fetchFn: typeof fetch;
  nowSecs: () => number;
  ttlSecs: number;
}

function defaults(): CallerIdentityDeps {
  return {
    issuer: process.env.CSS_ISSUER || 'https://id.lightlifeurbangardens.com/',
    jwksUrl: process.env.CHORUS_JWKS_URL || 'http://localhost:3001/.oidc/jwks',
    fetchFn: fetch,
    nowSecs: () => Math.floor(Date.now() / 1000),
    ttlSecs: 300,
  };
}

let deps: CallerIdentityDeps = defaults();
let jwks: { keys: Map<string, crypto.KeyObject>; expires: number } | null = null;

/** Tests swap the issuer, JWKS source and clock; no argument restores the defaults. */
export function configureCallerIdentity(over?: Partial<CallerIdentityDeps>): void {
  deps = { ...defaults(), ...(over ?? {}) };
  jwks = null;
}

function b64urlJson(part: string): Record<string, unknown> | null {
  try {
    return JSON.parse(Buffer.from(part, 'base64url').toString('utf-8'));
  } catch {
    return null;
  }
}

/** CSS serves .oidc routes only as the logical issuer; a loopback fetch carries
 *  the trusted-proxy headers (#3720, same as es256-identity.ts). */
function hairpinHeaders(): Record<string, string> {
  try {
    const iss = new URL(deps.issuer);
    if (new URL(deps.jwksUrl).host === iss.host) return {};
    return { Host: iss.host, 'X-Forwarded-Proto': iss.protocol.replace(':', ''), 'X-Forwarded-Host': iss.host };
  } catch {
    return {};
  }
}

async function fetchKeys(): Promise<Map<string, crypto.KeyObject> | null> {
  try {
    const res = await deps.fetchFn(deps.jwksUrl, { headers: hairpinHeaders() });
    if (!res.ok) return null;
    const body = (await res.json()) as { keys?: Array<Record<string, string>> };
    const keys = new Map<string, crypto.KeyObject>();
    for (const jwk of body.keys ?? []) {
      if (jwk.kty !== 'EC' || !jwk.kid) continue;
      try {
        keys.set(jwk.kid, crypto.createPublicKey({ key: jwk, format: 'jwk' }));
      } catch {
        /* one bad key must not blind the check to the rest */
      }
    }
    return keys;
  } catch {
    return null;
  }
}

/** A cached key while fresh; else refetch. With no fresh keys, fail closed. */
async function keyFor(kid: string): Promise<crypto.KeyObject | null> {
  const now = deps.nowSecs();
  const cached = jwks !== null && jwks.expires > now ? jwks.keys : null;
  if (cached?.has(kid)) return cached.get(kid) ?? null;
  const keys = await fetchKeys();
  if (!keys) return cached?.get(kid) ?? null;
  jwks = { keys, expires: now + deps.ttlSecs };
  return keys.get(kid) ?? null;
}

/** The role a WebID names: https://<issuer host>/<role>/profile/card#me. */
export function roleFromWebId(webId: string, issuer: string): string | null {
  let u: URL;
  let iss: URL;
  try {
    u = new URL(webId);
    iss = new URL(issuer);
  } catch {
    return null;
  }
  if (u.host !== iss.host || u.hash !== '#me') return null;
  const m = /^\/([a-z][a-z0-9-]{0,31})\/profile\/card$/.exec(u.pathname);
  return m ? m[1] : null;
}

/** The token's claims when its ES256 signature verifies against the issuer's keys. */
async function verifiedClaims(token: string): Promise<Record<string, unknown> | null> {
  const parts = token.split('.');
  if (parts.length !== 3) return null;
  const head = b64urlJson(parts[0]);
  if (head?.alg !== 'ES256' || typeof head.kid !== 'string' || !head.kid) return null;
  const key = await keyFor(head.kid);
  if (!key) return null;
  const sigOk = crypto.verify(
    'sha256',
    Buffer.from(`${parts[0]}.${parts[1]}`),
    { key, dsaEncoding: 'ieee-p1363' },
    Buffer.from(parts[2], 'base64url'),
  );
  return sigOk ? b64urlJson(parts[1]) : null;
}

const sameIssuer = (a: string, b: string) => a.replace(/\/+$/, '') === b.replace(/\/+$/, '');

/** The role an `Authorization: Bearer <token>` header proves, or null. */
export async function callerRole(authorization: string | undefined): Promise<string | null> {
  const m = /^Bearer\s+(\S+)$/.exec(authorization ?? '');
  const claims = m ? await verifiedClaims(m[1]) : null;
  if (!claims || typeof claims.iss !== 'string' || !sameIssuer(claims.iss, deps.issuer)) return null;
  if (typeof claims.exp !== 'number' || claims.exp <= deps.nowSecs()) return null;
  return typeof claims.webid === 'string' ? roleFromWebId(claims.webid, deps.issuer) : null;
}
