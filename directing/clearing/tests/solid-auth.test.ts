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

import { isWebIdAllowed, authenticateSolid, _resetAllowCache } from '../src/solid-auth';

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
