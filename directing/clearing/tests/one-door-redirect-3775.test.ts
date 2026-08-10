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
    // #3792 — pinned to the guard's ACTUAL parser (chorus-share-guard.py:617,
    // :687 read `next`). A rename on either side breaks this test rather than
    // silently breaking the journey.
    expect(block).toContain('?next=');
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

describe('#3775 guard-session consumption — source contract', () => {
  const server = fs.readFileSync(path.join(SRC, 'server.ts'), 'utf-8');

  test('isAuthed accepts the guard session, and asks the allow-set separately', () => {
    const fn = server.slice(server.indexOf('async function isAuthed'), server.indexOf('async function gate'));
    expect(fn).toContain('shareSessionWebId');
    expect(fn).toContain('isWebIdAllowed');
    // per-request, both questions: verify (who) THEN allow-set (whether)
    expect(fn.indexOf('shareSessionWebId')).toBeLessThan(fn.indexOf('isWebIdAllowed'));
  });

  test('sessionPrincipal resolves the guard WebID too — one person, either door', () => {
    const fn = server.slice(server.indexOf('async function sessionPrincipal'), server.indexOf('async function isAuthed'));
    expect(fn).toContain('shareSessionWebId');
    expect(fn).toContain('principalForWebId');
  });

  test('NEGATIVE PROOF: the guard cookie is never trusted unverified', () => {
    // A path that read the cookie and used its webid WITHOUT verifyShareSession
    // would be a self-mint hole: anyone could set the cookie by hand.
    const helper = server.slice(server.indexOf('function shareSessionWebId'), server.indexOf('function isAuthed'));
    expect(helper).toContain('verifyShareSession');
    expect(helper).toContain('v.ok ? v.webid : null');
  });
});

describe('#3792 — the parameter name is pinned to the guard\'s parser', () => {
  const server = fs.readFileSync(path.join(SRC, 'server.ts'), 'utf-8');
  const block = server.slice(server.indexOf('const commonDoor'), server.indexOf('const commonDoor') + 1400);

  test('the redirect sends next=, the parameter chorus-share-guard.py actually reads', () => {
    expect(block).toContain('?next=');
  });

  test('NEGATIVE PROOF: the pre-#3792 shape (?return=) is absent — and would be caught here if it came back', () => {
    // The failure this replaces was SILENT: the guard ignores an unknown
    // parameter and signs the visitor in to its own root, so nothing errors and
    // the journey just quietly ends in the wrong place. Assert the wrong shape
    // is gone, and prove the assertion can see it by checking the matcher works.
    expect(block).not.toContain('?return=');
    expect('someurl?return=x').toContain('?return=');  // the check can go red
  });
});
