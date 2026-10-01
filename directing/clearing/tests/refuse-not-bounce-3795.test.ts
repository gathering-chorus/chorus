// @test-type: unit:security — drives the real gate over HTTP in-process and runs the real verifier; temp key, stub allow-set store, no live guard.
// @card: #3795
// @owner: wren
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
import { useInProcessClearing } from './lib/in-process-clearing'; // first: sets the temp guard key path
import { verifyShareSession } from '../src/share-session';

const fixture = JSON.parse(
  fs.readFileSync(path.join(__dirname, 'fixtures', 'session-cookie-vectors.json'), 'utf-8'),
) as { key_b64u: string; now: number; vectors: { name: string; cookie: string; expect: string }[] };
const KEY = Buffer.from(fixture.key_b64u.replace(/-/g, '+').replace(/_/g, '/'), 'base64');

// #4417 — the gate is driven over HTTP now, not read as source text.
const { visit, guardCookie, STRANGER } = useInProcessClearing();

describe('#3795 the refusal branch, driven', () => {
  test('a verified identity not on the list is refused by name: 403, never a 3xx', async () => {
    process.env.CHORUS_SIGNIN_URL = 'https://door.example/signin';
    const r = await visit('GET', '/', guardCookie(STRANGER));
    expect(r.status).toBe(403);
    expect(r.location).toBeNull();
    expect(r.body).toContain('pods.example/stranger');
    expect(r.body).toContain('/logout');
    expect(r.body).toContain('Sign out');
    expect(r.body).not.toContain('/auth/login'); // a sign-in link here IS the loop
  });

  test('NEGATIVE PROOF: a tampered cookie is nobody, sent to the door, not refused by name', async () => {
    process.env.CHORUS_SIGNIN_URL = 'https://door.example/signin';
    const good = guardCookie(STRANGER);
    const tampered = good.slice(0, -2) + (good.endsWith('AA') ? 'BB' : 'AA');
    const r = await visit('GET', '/', tampered);
    expect(r.status).toBe(302);
    expect(r.location).toContain('https://door.example/signin?next=');
    expect(r.body).not.toContain('pods.example/stranger');
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
