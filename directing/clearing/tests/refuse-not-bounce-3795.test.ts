// @test-type: security — exercises the REAL gate's refusal branch via the exported source contract + the real verifier; fixture key, no live guard, brings its own world.
/**
 * #3795 — refused is not unknown.
 *
 * Silas's live walk (2026-08-08) as an identity deliberately left off the
 * allow-set: login → callback → login → callback, eight times, until Chrome
 * gave up. The Clearing redirected a session it had VERIFIED, the guard saw a
 * valid cookie and re-minted it, and the two correct doors spun forever.
 *
 * The rule these tests hold: a session we cannot verify is NOBODY (redirect to
 * the door). A session we CAN verify, for a person this app does not admit, is
 * a KNOWN PERSON TOLD NO (refuse — never a 3xx).
 */
import fs from 'fs';
import path from 'path';
import { verifyShareSession } from '../src/share-session';

const SRC = path.join(__dirname, '..', 'src');
const server = fs.readFileSync(path.join(SRC, 'server.ts'), 'utf-8');
const fixture = JSON.parse(
  fs.readFileSync(path.join(__dirname, 'fixtures', 'session-cookie-vectors.json'), 'utf-8'),
) as { key_b64u: string; now: number; vectors: { name: string; cookie: string; expect: string }[] };
const KEY = Buffer.from(fixture.key_b64u.replace(/-/g, '+').replace(/_/g, '/'), 'base64');

describe('#3795 the refusal branch', () => {
  const gate = server.slice(server.indexOf('async function gate'), server.indexOf('function handleAuthLogin'));

  test('a verified identity is refused with 403 BEFORE any redirect is considered', () => {
    const refuseAt = gate.indexOf('res.status(403).send(refusedPage');
    const redirectAt = gate.indexOf('res.redirect(`${commonDoor}');
    expect(refuseAt).toBeGreaterThan(-1);
    expect(redirectAt).toBeGreaterThan(-1);
    // order is the whole fix: refuse first, so a verified session never reaches
    // the redirect that the guard would answer by re-minting.
    expect(refuseAt).toBeLessThan(redirectAt);
  });

  test('NEGATIVE PROOF: the refusal is not a 3xx — the loop cannot come back as a redirect', () => {
    const branch = gate.slice(gate.indexOf('const knownWebId'), gate.indexOf('const commonDoor'));
    expect(branch).toContain('403');
    expect(branch).not.toMatch(/res\.redirect/);
    // and prove the matcher can see a redirect if one were there
    expect('res.redirect(x)').toMatch(/res\.redirect/);
  });

  test('the refusal page names the identity and offers sign-out, with no path back to sign-in', () => {
    // slice exactly refusedPage — the next function along (errorPage) legitimately
    // links to /auth/login, and an over-wide window would fail on ITS content
    // rather than this page's, which is a check that cannot tell two functions apart.
    const start = server.indexOf('function refusedPage');
    const page = server.slice(start, server.indexOf('\n}', start) + 2);
    expect(page).toContain('/logout');
    expect(page).toContain('Sign out');
    expect(page).not.toContain('/auth/login');  // a sign-in link here IS the loop
  });

  test('the no-session path still redirects — unknown people still reach the door', () => {
    const after = gate.slice(gate.indexOf('const commonDoor'));
    expect(after).toContain('res.redirect');
    expect(after).toContain('?next=');
  });
});

describe('#3795 the verifier still separates the two states it must', () => {
  test('a valid cookie verifies (so the gate can tell "known" from "nobody")', () => {
    const valid = fixture.vectors.find((v) => v.name === 'valid')!;
    const v = verifyShareSession(valid.cookie, KEY, fixture.now);
    // Not `if (v.ok) expect(...)`: a conditional expect asserts nothing on the
    // branch it does not take, so a verifier that refused everything would pass
    // this test in silence — which is the exact defect class this card is about.
    if (!v.ok) throw new Error(`the valid vector failed to verify: ${v.reason}`);
    expect(v.webid).toMatch(/^https?:\/\//);
  });

  test('a tampered cookie does NOT verify (so those people are still sent to the door, not refused by name)', () => {
    const bad = fixture.vectors.find((v) => v.name === 'tampered-mac')!;
    expect(verifyShareSession(bad.cookie, KEY, fixture.now).ok).toBe(false);
  });
});
