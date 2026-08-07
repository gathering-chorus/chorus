// @test-type: security — asserts the REAL verifier against the shared cross-implementation vectors (test key, fixed clock); no live guard, no production key, brings its own world.
/**
 * #3775 — the Clearing's half of the guard session contract.
 *
 * The vectors file is Silas's (#3790) and is CANONICAL: one artifact, two
 * consumers — his Python verifier and this TypeScript one assert the same
 * verdicts. Clause 2 of DEC-2209 asks for a single shared verifier; until
 * #3685's crate lands there are two implementations of one contract, and this
 * turns drift between them into a red test instead of a login that works on one
 * surface and not the other.
 *
 * Asserted against the ACTUAL verifier the Clearing runs — never a copy written
 * for the test, which would agree with the original while both were wrong.
 */
import fs from 'fs';
import path from 'path';
import { verifyShareSession, shareSessionFromHeader, readCookieKey, SHARE_COOKIE_NAME } from '../src/share-session';

interface Vector { name: string; cookie: string; expect: 'accept' | 'reject'; why?: string }
const fixture = JSON.parse(
  fs.readFileSync(path.join(__dirname, 'fixtures', 'session-cookie-vectors.json'), 'utf-8'),
) as { key_b64u: string; now: number; vectors: Vector[] };

const KEY = Buffer.from(fixture.key_b64u.replace(/-/g, '+').replace(/_/g, '/'), 'base64');
const NOW = fixture.now;
const named = (n: string): Vector => fixture.vectors.find((v) => v.name === n)!;

describe('#3790 shared vectors — the Clearing verifier agrees with the guard', () => {
  test('every vector resolves to the verdict the fixture declares', () => {
    const got = fixture.vectors.map((v) => ({
      name: v.name,
      verdict: verifyShareSession(v.cookie, KEY, NOW).ok ? 'accept' : 'reject',
    }));
    expect(got).toEqual(fixture.vectors.map((v) => ({ name: v.name, verdict: v.expect })));
  });

  test('the valid vector yields a WebID — accept means we learned WHO, not merely that nothing threw', () => {
    const v = verifyShareSession(named('valid').cookie, KEY, NOW);
    expect(v.ok).toBe(true);
    if (v.ok) expect(v.webid).toMatch(/^https?:\/\//);
  });

  test('tampered-payload is refused — a stolen MAC on a swapped identity must not verify as that identity', () => {
    // The real attack, and the one a mac-over-the-wrong-string verifier accepts:
    // it would hand back the WRONG person, verified.
    const v = verifyShareSession(named('tampered-payload').cookie, KEY, NOW);
    expect(v.ok).toBe(false);
    if (!v.ok) expect(v.reason).toBe('bad-signature');
  });

  test('malformed input REFUSES rather than throws — a throw becomes a 500, and 500s get retried', () => {
    for (const n of ['payload-not-json', 'no-separator', 'empty']) {
      expect(() => verifyShareSession(named(n).cookie, KEY, NOW)).not.toThrow();
      expect(verifyShareSession(named(n).cookie, KEY, NOW).ok).toBe(false);
    }
  });

  test('NEGATIVE PROOF: the valid vector under a DIFFERENT key is refused — the fixture can fail a wrong verifier', () => {
    // Without this, every reject-case would also pass against an
    // accept-nothing implementation and the suite would prove nothing.
    const wrong = Buffer.from('a-different-key-entirely-not-the-fixture-key');
    const v = verifyShareSession(named('valid').cookie, wrong, NOW);
    expect(v.ok).toBe(false);
    if (!v.ok) expect(v.reason).toBe('bad-signature');
  });
});

describe('#3775 verifier properties beyond the vectors', () => {
  const valid = named('valid').cookie;

  test('no key fails closed — a lost key means signed out, never trusted-by-default', () => {
    const v = verifyShareSession(valid, null, NOW);
    expect(v.ok).toBe(false);
    if (!v.ok) expect(v.reason).toBe('no-key');
  });

  test('expiry is a boundary: accepted just before, refused just after', () => {
    const before = verifyShareSession(valid, KEY, NOW);
    expect(before.ok).toBe(true);
    const after = verifyShareSession(valid, KEY, NOW + 10 * 365 * 24 * 3600);
    expect(after.ok).toBe(false);
    if (!after.ok) expect(after.reason).toBe('expired');
  });

  test('verify answers WHO only — a signed cookie for an unknown WebID still verifies', () => {
    // Behavioral, not textual: the fixture's WebID is on no allow-set in this
    // process, and verification still succeeds. Authorization is asked
    // separately, per request — folding it in here would freeze the decision at
    // sign-in, the thing DEC-2209 removed.
    // (First version of this test grepped the source for 'isWebIdAllowed' and
    // failed on the COMMENT that explains this very separation — a check that
    // cannot tell a comment from a call, the same trap Silas hit in his
    // recovery-path test the same morning.)
    const v = verifyShareSession(valid, KEY, NOW);
    expect(v.ok).toBe(true);
    if (v.ok) {
      expect(typeof v.webid).toBe('string');
      expect(v).not.toHaveProperty('allowed');
      expect(v).not.toHaveProperty('role');
      expect(v).not.toHaveProperty('scope');
    }
  });

  test('the cookie is found among others, and absent when absent', () => {
    expect(shareSessionFromHeader(`a=1; ${SHARE_COOKIE_NAME}=${valid}; clearing_session=z`)).toBe(valid);
    expect(shareSessionFromHeader('nothing=here')).toBeUndefined();
    expect(shareSessionFromHeader(undefined)).toBeUndefined();
  });

  test('a missing state file yields no key, and the verifier turns that into a refusal', () => {
    const k = readCookieKey('/nonexistent/nowhere.json');
    expect(k).toBeNull();
    expect(verifyShareSession(valid, k, NOW).ok).toBe(false);
  });
});
