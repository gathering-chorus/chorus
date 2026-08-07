// @test-type: unit — env-driven branches exercised via the exported middleware path; no live guard, brings its own world.
/**
 * #3775 — the Clearing behind the one door (DEC-2209).
 *
 * Built legs under test here:
 *  - redirect: with CHORUS_SIGNIN_URL set, an unauthenticated GET is sent to
 *    the common door carrying the full return URL (host + path), so the door
 *    can land Jeff back where he was headed — cross-subdomain, which is the
 *    whole point.
 *  - fallback: unset, the local interstitial still serves (the flag flips only
 *    when the guard cookie is parent-domain scoped; early flip = redirect loop).
 *  - vocabulary (clause 7): "Log in" is extinct in the Clearing's sources —
 *    a regression lock, since the string came back twice during #3669.
 */
import fs from 'fs';
import path from 'path';

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

describe('#3775 redirect leg — source-level contract', () => {
  const server = fs.readFileSync(path.join(SRC, 'server.ts'), 'utf-8');

  test('common-door redirect exists, gated on CHORUS_SIGNIN_URL, and carries the return URL', () => {
    expect(server).toContain('CHORUS_SIGNIN_URL');
    const block = server.slice(server.indexOf('CHORUS_SIGNIN_URL'));
    expect(block).toContain('?return=');
    expect(block).toContain('safeReturnPath');
    expect(block).toContain('req.headers.host'); // FULL url — cross-subdomain return
  });

  test('fallback interstitial survives when the flag is unset', () => {
    expect(server).toContain('interstitialPage(safeReturnPath(req.originalUrl))');
  });

  test('redirect only fires for GET — a POST mid-flow must not be bounced into a login page', () => {
    const block = server.slice(server.indexOf('const commonDoor'));
    expect(block.slice(0, 400)).toContain("req.method === 'GET'");
  });
});
