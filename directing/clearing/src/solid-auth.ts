/**
 * #3669 lane 2 — Solid-OIDC identity at the Clearing's public door.
 *
 * Decided with Wren (2026-07-23): verify the DPoP token IN-PROCESS (local crypto,
 * no per-message round-trip) via @solid/access-token-verifier against the CSS
 * JWKS, then check the resulting WebID against the seam's allow-set — which per
 * ADR-052 constraint 4 IS `Principal.webId` alone. We read that set from the
 * security graph with a ~60s cache: revocation is a model edit that propagates
 * within one TTL, so policy still lives in ONE place (Wren's graph), and we
 * duplicate no policy logic — only membership (webid ∈ set).
 *
 * The public door REQUIRES DPoP (no bearer fallback): #3613 skipped DPoP under
 * the loopback bound, and going public is exactly when that residual bites, so
 * bearer is refused on tunneled requests. The static bridge token survives only
 * as a LOCAL / migration credential and is cut per-agent (auditable via spine).
 */

import { createSolidTokenVerifier } from '@solid/access-token-verifier';

/** The HTTP methods the DPoP verifier accepts for the `htm`/`method` check. */
type HttpMethod = 'GET' | 'CONNECT' | 'DELETE' | 'HEAD' | 'OPTIONS' | 'PATCH' | 'POST' | 'PUT' | 'TRACE';

/** Result of authenticating one request. */
export type AuthResult =
  | { ok: true; webid: string }
  | { ok: false; reason: 'no-credentials' | 'bad-token' | 'not-allowed' | 'bearer-refused' };

const verifySolidToken = createSolidTokenVerifier();

// --- allow-set: Principal.webId from the security graph, TTL-cached ----------

const FUSEKI_QUERY =
  process.env.CHORUS_FUSEKI_QUERY || 'http://localhost:3030/pods/query';
const ALLOW_TTL_MS = 60_000;
/**
 * #3669 (Wren, gemba catch) — MUST be scoped to the security graph the seam
 * governs (`urn:chorus:domains:security`, matching owl-api oidc.rs's
 * PRINCIPAL_ALLOW_QUERY). An unbound `GRAPH ?g` reads Principals from ANY graph,
 * and Fuseki still takes anonymous LAN writes (#3564) — so any local writer could
 * INSERT a Principal triple into a scratch graph and mint themselves into the
 * allow-set. Membership must come only from the graph the seam controls.
 */
export const ALLOW_QUERY =
  'PREFIX chorus: <https://jeffbridwell.com/chorus#> ' +
  'SELECT ?webid WHERE { GRAPH <urn:chorus:domains:security> { ?p a chorus:Principal ; chorus:webId ?webid } }';

let allowCache: { at: number; set: Set<string> } | null = null;

/** Fetch the allow-set (Principal WebIDs) from the seam's graph. */
async function fetchAllowSet(fetchImpl: typeof fetch = fetch): Promise<Set<string>> {
  const url = `${FUSEKI_QUERY}?query=${encodeURIComponent(ALLOW_QUERY)}`;
  const res = await fetchImpl(url, { headers: { Accept: 'application/sparql-results+json' } });
  if (!res.ok) throw new Error(`allow-set query ${res.status}`);
  const body = (await res.json()) as { results: { bindings: Array<{ webid: { value: string } }> } };
  return new Set(body.results.bindings.map((b) => b.webid.value));
}

/**
 * Is `webid` in the seam allow-set? Cached ALLOW_TTL_MS. On a cold-miss fetch
 * error the cache (if any) still serves — degraded-graceful, as Wren specified;
 * with no cache at all we fail CLOSED (deny), never open.
 */
export async function isWebIdAllowed(
  webid: string,
  now: number,
  fetchImpl: typeof fetch = fetch,
): Promise<boolean> {
  if (!allowCache || now - allowCache.at >= ALLOW_TTL_MS) {
    try {
      allowCache = { at: now, set: await fetchAllowSet(fetchImpl) };
    } catch {
      if (!allowCache) return false; // cold-miss + seam down → deny (fail closed)
      // else: serve the stale set (degraded-graceful)
    }
  }
  return allowCache.set.has(webid);
}

/** Test seam — reset the module cache. */
export function _resetAllowCache(): void {
  allowCache = null;
}

// --- request authentication --------------------------------------------------

/**
 * Authenticate one request from its headers. `requireDpop` is true for the
 * public door (tunneled): a bearer token is refused outright. Returns the
 * verified WebID (allow-set-checked) or a typed refusal. Verification failures
 * never throw — they resolve to `{ ok:false }`.
 */
export async function authenticateSolid(
  authorization: string | undefined,
  dpop: string | undefined,
  method: string,
  url: string,
  requireDpop: boolean,
  now: number,
  fetchImpl: typeof fetch = fetch,
): Promise<AuthResult> {
  if (!authorization) return { ok: false, reason: 'no-credentials' };
  const scheme = authorization.split(' ', 1)[0]?.toLowerCase();
  if (requireDpop && scheme !== 'dpop') return { ok: false, reason: 'bearer-refused' };

  let webid: string;
  try {
    // The verifier types `method` as the HTTP-method union; callers pass a real
    // request method, so the cast is sound.
    const dpopOpts = dpop
      ? { header: dpop, method: method.toUpperCase() as HttpMethod, url }
      : undefined;
    const token = await verifySolidToken(authorization, dpopOpts);
    webid = token.webid;
  } catch {
    return { ok: false, reason: 'bad-token' };
  }
  if (!webid) return { ok: false, reason: 'bad-token' };
  if (!(await isWebIdAllowed(webid, now, fetchImpl))) return { ok: false, reason: 'not-allowed' };
  return { ok: true, webid };
}
