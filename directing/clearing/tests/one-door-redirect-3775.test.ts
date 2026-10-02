// @test-type: unit — drives the in-process Clearing over HTTP with a stub allow-set store and a temp guard key; brings its own world.
// @card: #3775
// @owner: wren
/**
 * #3775 — the Clearing behind the one door (DEC-2209).
 *
 * Built legs under test here:
 *  - redirect: with CHORUS_SIGNIN_URL set, an unauthenticated GET is sent to
 *    the common door carrying the full return URL (host + path), so the door
 *    can land Jeff back where he was headed — cross-host, which is the
 *    whole point.
 *  - fallback: unset, the local interstitial still serves (the flag flips only
 *    when the guard cookie is parent-domain scoped; early flip = redirect loop).
 *  - vocabulary (clause 7): "Log in" is extinct in the Clearing's sources —
 *    a regression lock, since the string came back twice during #3669. This
 *    is a copy check over pages, so it reads files on purpose.
 *
 * #4417 — the redirect and guard-session legs used to match server.ts text.
 * They now send real requests to the in-process server: tunneled (cf-ray), so
 * the loopback exemption does not apply, with guard cookies signed by a temp
 * key and an allow-set served by a stub store.
 */
import fs from 'fs';
import path from 'path';
import crypto from 'crypto';
import { useInProcessClearing } from './lib/in-process-clearing';

const SRC = path.join(__dirname, '..', 'src');
const PUB = path.join(__dirname, '..', 'public');

describe('#3775 vocabulary — Sign in, never Log in', () => {
  test('no source or page ships the words "Log in" / "LOG IN"', () => {
    const offenders: string[] = [];
    const scan = (dir: string, exts: string[]) => {
      for (const f of fs.readdirSync(dir)) {
        const p = path.join(dir, f);
        if (fs.statSync(p).isDirectory()) continue;
        if (!exts.some((e) => f.endsWith(e))) continue;
        const body = fs.readFileSync(p, 'utf-8');
        // "login" as an identifier/path is fine; the human-facing phrase is not.
        if (/Log in|LOG IN/.test(body)) offenders.push(f);
      }
    };
    scan(SRC, ['.ts']);
    scan(PUB, ['.html', '.js']);
    expect(offenders).toEqual([]);
  });

  test('NEGATIVE PROOF: the scanner sees the phrase when planted', () => {
    // a check that cannot go red must not gate (#3734): the regex must actually
    // match the phrase it polices.
    expect(/Log in|LOG IN/.test('<button>Log in</button>')).toBe(true);
    expect(/Log in|LOG IN/.test('handleAuthLogin login clearing_login')).toBe(false);
  });
});

const gate = useInProcessClearing();
const { visit, guardCookie, ALLOWED } = gate;

describe('#3775 the one door, driven', () => {
  test('no session, door set: GET is sent to the door with the full return URL', async () => {
    process.env.CHORUS_SIGNIN_URL = 'https://door.example/signin';
    const r = await visit('GET', '/stream');
    expect(r.status).toBe(302);
    expect(r.location).toBe(`https://door.example/signin?next=${encodeURIComponent('https://team.example.com/stream')}`);
  });

  test('no session, door unset: the local sign-in page serves, no redirect', async () => {
    delete process.env.CHORUS_SIGNIN_URL;
    const r = await visit('GET', '/');
    expect(r.status).toBe(401);
    expect(r.location).toBeNull();
    expect(r.body).toContain('Sign in');
  });

  test('no session, door set: a POST is refused, never bounced to a sign-in page', async () => {
    process.env.CHORUS_SIGNIN_URL = 'https://door.example/signin';
    const r = await visit('POST', '/api/message');
    expect(r.status).toBe(401);
    expect(r.location).toBeNull();
  });

  test('NEGATIVE PROOF: a guard cookie signed with the wrong key is nobody, sent to the door', async () => {
    process.env.CHORUS_SIGNIN_URL = 'https://door.example/signin';
    const r = await visit('GET', '/', guardCookie(ALLOWED, crypto.randomBytes(32)));
    expect(r.status).toBe(302);
    expect(r.location).toContain('https://door.example/signin?next=');
  });

  test('a verified guard session on the allow-set gets the room', async () => {
    process.env.CHORUS_SIGNIN_URL = 'https://door.example/signin';
    const r = await visit('GET', '/', guardCookie(ALLOWED));
    expect(r.status).toBe(200);
    expect(r.location).toBeNull();
  });
  // #3795's refusal (verified, not on the list) is driven in refuse-not-bounce-3795.
});
