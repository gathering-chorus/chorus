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
 * #3785 — THE ONE NAME, matching chorus-oidc's `allow_set_graph()`.
 *
 * On 2026-08-06 this string was spelled by hand in twelve places across Rust,
 * TypeScript, shell and the ontology files. The Principal records were moved to
 * the graph owl-api SERVES from, the copies in the graph the doors READ were
 * retired as duplicates — and they were not duplicates, they were the same
 * records with two consumers. Jeff was locked out of the Clearing in minutes.
 *
 * Env-overridable so the graph can move by configuration rather than by a
 * coordinated edit across four languages, which is the move that broke it.
 */
export const ALLOW_SET_GRAPH =
  process.env.CHORUS_ALLOW_SET_GRAPH || 'urn:chorus:domains:security';

/**
 * How long a previously-verified allow-set may keep serving while the store is
 * unreachable, before the door fails closed anyway.
 *
 * Wren's distinction (2026-08-06), which resolved two ACs that were arguing with
 * each other: fail-closed governs UNVERIFIABLE ASSERTIONS — a cookie that does
 * not verify, a lost key. It does not govern infrastructure hiccups on a set we
 * already verified, because a store hiccup is not a revocation, and treating one
 * as the other is the lockout class we hit twice in a day.
 *
 * The bound is why this is safe: past the ceiling we fail closed regardless,
 * and an attacker who can hold the store down that long has deeper access than
 * the allow-set anyway. In config, not code — a security bound that needs a
 * rebuild to change is one nobody adjusts under pressure.
 */
export const STALE_CEILING_MS = Number(
  process.env.CHORUS_ALLOW_STALE_CEILING_MS || 5 * 60_000,
);

/** Emitted at refresh and on degraded serves: the line missing on 2026-08-06,
 *  when two consumers read two graphs and no surface named its source. */
export function graphProvenance(count: number, reason: string): string {
  return `allow-set: ${count} principal webid(s) from <${ALLOW_SET_GRAPH}> (${reason})`;
}
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
  `SELECT ?webid WHERE { GRAPH <${ALLOW_SET_GRAPH}> { ?p a chorus:Principal ; chorus:webId ?webid } }`;

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
      console.log(graphProvenance(allowCache.set.size, 'refreshed'));
    } catch {
      if (!allowCache) return false; // cold-miss + seam down → deny (fail closed)
      // #3785 — a store hiccup is not a revocation, so a previously-verified set
      // keeps serving. BOUNDED: past the ceiling we fail closed anyway. The age
      // travels in the alarm because "40 seconds stale" and "4 minutes stale"
      // are different facts, and only one of them is near the cliff — without it
      // whoever reads this at 3am cannot tell how much time they have.
      const ageMs = now - allowCache.at;
      if (ageMs >= STALE_CEILING_MS) {
        console.error(
          graphProvenance(allowCache.set.size,
            `STALE ${Math.round(ageMs / 1000)}s — past the ${Math.round(STALE_CEILING_MS / 1000)}s ceiling, FAILING CLOSED`),
        );
        return false;
      }
      console.error(
        graphProvenance(allowCache.set.size,
          `store unreachable — serving last known-good, ${Math.round(ageMs / 1000)}s old, ceiling ${Math.round(STALE_CEILING_MS / 1000)}s`),
      );
    }
  }
  return allowCache.set.has(webid);
}

// --- principal map: WebID → Principal, same graph, same TTL (#3743) ----------

/**
 * #3743 — display identity and authority come from the Principal the verified
 * WebID belongs to, same security graph the allow-set reads (and only that
 * graph, for the #3669 reason above). `name` is the principal's local id minus
 * the `principal-` prefix (principal-marknakib → marknakib).
 */
export const PRINCIPAL_MAP_QUERY =
  'PREFIX chorus: <https://jeffbridwell.com/chorus#> ' +
  `SELECT ?p ?webid WHERE { GRAPH <${ALLOW_SET_GRAPH}> { ?p a chorus:Principal ; chorus:webId ?webid } }`;

let principalCache: { at: number; map: Map<string, { id: string; name: string }> } | null = null;

async function fetchPrincipalMap(fetchImpl: typeof fetch = fetch): Promise<Map<string, { id: string; name: string }>> {
  const url = `${FUSEKI_QUERY}?query=${encodeURIComponent(PRINCIPAL_MAP_QUERY)}`;
  const res = await fetchImpl(url, { headers: { Accept: 'application/sparql-results+json' } });
  if (!res.ok) throw new Error(`principal-map query ${res.status}`);
  const body = (await res.json()) as {
    results: { bindings: Array<{ p: { value: string }; webid: { value: string } }> };
  };
  const map = new Map<string, { id: string; name: string }>();
  for (const b of body.results.bindings) {
    const id = b.p.value.split(/[#/]/).pop() || '';
    if (!id) continue;
    map.set(b.webid.value, { id, name: id.replace(/^principal-/, '') });
  }
  return map;
}

/**
 * Principal for a verified WebID, or null. Cache semantics mirror
 * isWebIdAllowed: TTL'd, stale-served on fetch error, fail-closed (null) on a
 * cold miss — an unresolvable principal must never resolve to an identity.
 */
export async function principalForWebId(
  webid: string,
  now: number,
  fetchImpl: typeof fetch = fetch,
): Promise<{ id: string; name: string } | null> {
  if (!principalCache || now - principalCache.at >= ALLOW_TTL_MS) {
    try {
      principalCache = { at: now, map: await fetchPrincipalMap(fetchImpl) };
    } catch {
      if (!principalCache) return null; // cold-miss + seam down → no identity
    }
  }
  return principalCache.map.get(webid) || null;
}

/**
 * #3785 — revocation is a DELIBERATE act and must bite on the next request, not
 * whenever a TTL happens to lapse. A write to the principal set calls this, so
 * the next read re-resolves.
 *
 * This is the half of the fail-closed story that is about intent rather than
 * infrastructure: removing someone is an assertion we can verify immediately,
 * unlike a store outage, which we cannot and must not treat as a removal.
 */
export function invalidateAllowCache(): void {
  allowCache = null;
  principalCache = null;
}

/** Test seam — reset the module cache. Delegates so the two cannot drift
 *  (Wren, #3785 review). */
export function _resetAllowCache(): void {
  invalidateAllowCache();
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
