// @test-type: security
/**
 * #3669 lane 2 — Solid-OIDC identity at the Clearing door.
 *
 * Covers the allow-set TTL cache (revocation-within-one-TTL, fail-closed on cold
 * seam-down, degraded-graceful on warm seam-down) and the request authenticator
 * (DPoP-required on the public door, allow-set gate). The @solid verifier is
 * mocked so these are deterministic unit tests of OUR policy, not the library's
 * crypto.
 */

const mockVerify = jest.fn();
jest.mock('@solid/access-token-verifier', () => ({
  createSolidTokenVerifier: () => mockVerify,
}));

import { isWebIdAllowed, authenticateSolid, _resetAllowCache, invalidateAllowCache, ALLOW_SET_GRAPH, ALLOW_QUERY, STALE_CEILING_MS } from '../src/solid-auth';

const WREN = 'http://localhost:3001/wren/profile/card#me';
const STRANGER = 'http://localhost:3001/mallory/profile/card#me';

function fakeFetch(webids: string[], opts: { fail?: boolean } = {}): typeof fetch {
  return (async () => {
    if (opts.fail) throw new Error('seam down');
    return {
      ok: true,
      json: async () => ({ results: { bindings: webids.map((w) => ({ webid: { value: w } })) } }),
    };
  }) as unknown as typeof fetch;
}

/**
 * A store fake that honors graph-scoping: `security` holds only allowed WebIDs;
 * `scratch` is what an anonymous LAN writer (#3564) could INSERT. The fetch reads
 * whichever set the outgoing query actually scopes to — so a code query bound to
 * urn:chorus:domains:security can NEVER see the scratch Principal.
 */
function graphAwareFetch(security: string[], scratch: string[]): typeof fetch {
  return (async (url: string) => {
    const scoped = decodeURIComponent(url).includes('<urn:chorus:domains:security>');
    const visible = scoped ? security : [...security, ...scratch];
    return {
      ok: true,
      json: async () => ({ results: { bindings: visible.map((w) => ({ webid: { value: w } })) } }),
    };
  }) as unknown as typeof fetch;
}

beforeEach(() => {
  mockVerify.mockReset();
  _resetAllowCache();
});

describe('#3669 allow-set is scoped to the seam-governed graph (Wren gemba catch)', () => {
  test('MUST-FIX: a Principal an attacker INSERTs into a non-security graph does NOT authorize', async () => {
    // Fuseki takes anon LAN writes (#3564). Mallory INSERTs her own Principal into
    // a scratch graph. isWebIdAllowed's query is bound to urn:chorus:domains:security,
    // so the scratch Principal is invisible → denied.
    const fetch = graphAwareFetch([WREN], [STRANGER]);
    expect(await isWebIdAllowed(WREN, 1000, fetch)).toBe(true); // real member
    _resetAllowCache();
    expect(await isWebIdAllowed(STRANGER, 1000, fetch)).toBe(false); // scratch-graph injection blocked
  });
});

describe('#3669 isWebIdAllowed — allow-set TTL cache', () => {
  test('a WebID in the seam allow-set is allowed', async () => {
    expect(await isWebIdAllowed(WREN, 1000, fakeFetch([WREN]))).toBe(true);
  });

  test('a WebID not in the allow-set is denied', async () => {
    expect(await isWebIdAllowed(STRANGER, 1000, fakeFetch([WREN]))).toBe(false);
  });

  test('cold-miss with the seam DOWN fails CLOSED (deny, never open)', async () => {
    expect(await isWebIdAllowed(WREN, 1000, fakeFetch([], { fail: true }))).toBe(false);
  });

  test('revocation propagates within one TTL (re-query after 60s)', async () => {
    // t=1000: wren allowed. t=1000+59s: still cached (allowed). t=1000+61s: seam
    // now returns empty (revoked) → denied.
    expect(await isWebIdAllowed(WREN, 1000, fakeFetch([WREN]))).toBe(true);
    expect(await isWebIdAllowed(WREN, 1000 + 59_000, fakeFetch([]))).toBe(true); // cache still warm
    expect(await isWebIdAllowed(WREN, 1000 + 61_000, fakeFetch([]))).toBe(false); // re-queried, revoked
  });

  test('warm cache + seam blip serves the stale set (degraded-graceful)', async () => {
    expect(await isWebIdAllowed(WREN, 1000, fakeFetch([WREN]))).toBe(true);
    // 61s later the seam is DOWN — the warm set still answers rather than denying.
    expect(await isWebIdAllowed(WREN, 1000 + 61_000, fakeFetch([], { fail: true }))).toBe(true);
  });
});

describe('#3669 authenticateSolid — the door', () => {
  test('valid DPoP token for an allowed WebID → ok', async () => {
    mockVerify.mockResolvedValue({ webid: WREN });
    const r = await authenticateSolid('DPoP tok', 'proof', 'GET', 'https://clearing.x/api', true, 1000, fakeFetch([WREN]));
    expect(r).toEqual({ ok: true, webid: WREN });
  });

  test('public door REFUSES a bearer token (DPoP required, #3613 residual)', async () => {
    const r = await authenticateSolid('Bearer tok', undefined, 'GET', 'https://clearing.x/api', true, 1000, fakeFetch([WREN]));
    expect(r).toEqual({ ok: false, reason: 'bearer-refused' });
    expect(mockVerify).not.toHaveBeenCalled();
  });

  test('no Authorization header → no-credentials', async () => {
    const r = await authenticateSolid(undefined, undefined, 'GET', 'https://clearing.x/api', true, 1000, fakeFetch([WREN]));
    expect(r).toEqual({ ok: false, reason: 'no-credentials' });
  });

  test('verifier throws (bad/expired/forged) → bad-token, never throws out', async () => {
    mockVerify.mockRejectedValue(new Error('bad dpop iat'));
    const r = await authenticateSolid('DPoP tok', 'proof', 'GET', 'https://clearing.x/api', true, 1000, fakeFetch([WREN]));
    expect(r).toEqual({ ok: false, reason: 'bad-token' });
  });

  test('valid token but WebID not in allow-set → not-allowed', async () => {
    mockVerify.mockResolvedValue({ webid: STRANGER });
    const r = await authenticateSolid('DPoP tok', 'proof', 'GET', 'https://clearing.x/api', true, 1000, fakeFetch([WREN]));
    expect(r).toEqual({ ok: false, reason: 'not-allowed' });
  });

  test('non-public door (requireDpop=false) still verifies, bearer allowed to reach verify', async () => {
    mockVerify.mockResolvedValue({ webid: WREN });
    const r = await authenticateSolid('Bearer tok', undefined, 'GET', 'http://localhost:3470/api', false, 1000, fakeFetch([WREN]));
    expect(r).toEqual({ ok: true, webid: WREN });
  });
});


// --- #3785: the allow-set has one home, and staleness is BOUNDED -------------
//
// Wren's distinction, which resolved two acceptance criteria that were arguing
// with each other: fail-closed governs UNVERIFIABLE ASSERTIONS — a cookie that
// does not verify, a lost key. It does not govern an infrastructure hiccup on a
// set we already verified. A store hiccup is not a revocation, and treating one
// as the other is the lockout class that hit Jeff twice on 2026-08-06.
//
// Three legs, and the THIRD is written first on purpose: "hold the store down
// past the ceiling and confirm refusal" is tedious to arrange and easy to
// assume, which is exactly why it is the only one that proves the ceiling is
// real rather than a number in a comment.

describe('#3785 — one graph name, bounded staleness', () => {
  beforeEach(() => _resetAllowCache());

  test('LEG 3 (the one that would never get written): past the ceiling, a warm cache is REFUSED', async () => {
    const t0 = 1_000_000;
    expect(await isWebIdAllowed(WREN, t0, fakeFetch([WREN]))).toBe(true);
    // store down, and we are past the ceiling: the bound must bite even though
    // the cached answer would have said yes.
    const past = t0 + STALE_CEILING_MS + 1;
    expect(await isWebIdAllowed(WREN, past, fakeFetch([], { fail: true }))).toBe(false);
  });

  test('LEG 2: inside the ceiling, a store hiccup keeps an existing session working', async () => {
    const t0 = 2_000_000;
    expect(await isWebIdAllowed(WREN, t0, fakeFetch([WREN]))).toBe(true);
    const inside = t0 + Math.floor(STALE_CEILING_MS / 2);
    expect(await isWebIdAllowed(WREN, inside, fakeFetch([], { fail: true }))).toBe(true);
  });

  test('LEG 1: revocation bites on the NEXT request, not at TTL expiry', async () => {
    const t0 = 3_000_000;
    expect(await isWebIdAllowed(WREN, t0, fakeFetch([WREN]))).toBe(true);
    // the deliberate act — a write to the principal set invalidates the cache
    invalidateAllowCache();
    // ...and the very next read re-resolves against a set that no longer names her
    expect(await isWebIdAllowed(WREN, t0 + 1, fakeFetch([]))).toBe(false);
  });

  test('NEGATIVE PROOF: without invalidation the stale answer would have persisted', async () => {
    // Proves LEG 1 is testing invalidation and not merely TTL expiry — without
    // this, a cache that ignored invalidateAllowCache() entirely would still pass
    // LEG 1 whenever the clock happened to cross the TTL.
    const t0 = 4_000_000;
    expect(await isWebIdAllowed(WREN, t0, fakeFetch([WREN]))).toBe(true);
    expect(await isWebIdAllowed(WREN, t0 + 1, fakeFetch([]))).toBe(true);
  });

  test('the allow-set query is scoped to the ONE declared graph', async () => {
    expect(ALLOW_QUERY).toContain(`GRAPH <${ALLOW_SET_GRAPH}>`);
  });

  test('NEGATIVE PROOF: an unscoped query would be a self-mint hole, and is not what we ship', async () => {
    // Fuseki still takes anonymous LAN writes (#3564), so an unbound GRAPH ?g
    // would let any local writer INSERT a Principal into a scratch graph and add
    // themselves to the allow-set.
    expect(ALLOW_QUERY).not.toMatch(/GRAPH\s+\?/);
  });
});
