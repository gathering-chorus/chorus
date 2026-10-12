// @test-type: unit — the Clearing over HTTP, in-process, every page under the /clearing mount; no live service.
// @card: #4412
// @owner: wren
/**
 * #4412 — Jeff, 2026-10-11 morning, on the apex (lightlifeurbangardens.com/clearing):
 * Account → Sign in gave a 404 on the Clearing's own sign-in (since removed, one
 * sign-in for Chorus is the door's); after signing in he landed on
 * "Cannot GET /account"; the background image 404'd on every load. One class: a
 * root-absolute link or redirect leaves the mount and lands on the apex.
 *
 * This walks the pages a person reaches, signed in and not, and fails on any link,
 * form, style url or redirect that leaves /clearing, and on any same-mount link
 * that answers 404. A page added later with a bare '/x' link goes red here.
 */
import { useInProcessClearing } from './lib/in-process-clearing'; // first: sets the temp guard key path

const gate = useInProcessClearing();

const LINK = /(?:src|href|action)="([^"]*)"|url\((['"]?)([^'")]*)\2\)|fetch\('([^']*)'/g;

function rootLinks(html: string): string[] {
  const out: string[] = [];
  for (const m of html.matchAll(LINK)) {
    const u = m[1] ?? m[3] ?? m[4] ?? '';
    if (u.startsWith('/') && !u.startsWith('//')) out.push(u);
  }
  return [...new Set(out)];
}

const leaving = (links: string[]) => links.filter((u) => u !== '/clearing' && !u.startsWith('/clearing/') && !u.startsWith('/clearing?'));

describe('#4412 every page under /clearing keeps its links inside the mount', () => {
  const pages: Array<[string, string, boolean]> = [
    ['the room, signed in', '/clearing/', true],
    ['the room, signed out (door not configured)', '/clearing/', false],
    ['account, signed in', '/clearing/account', true],
    ['account, signed out (door not configured)', '/clearing/account', false],
  ];

  for (const [label, path, signedIn] of pages) {
    test(`${label}: no link, form, style or fetch leaves /clearing`, async () => {
      const r = await gate.visit('GET', path, signedIn ? gate.guardCookie(gate.ALLOWED) : undefined);
      expect(r.status).not.toBe(404);
      const links = rootLinks(r.body);
      expect(links.length).toBeGreaterThan(0); // a page with no links proves nothing
      expect(leaving(links)).toEqual([]);
    });
  }

  test('#4412 signed out under the mount: the door is asked to bring him back inside /clearing', async () => {
    process.env.CHORUS_SIGNIN_URL = 'https://door.example/_auth/login';
    const r = await gate.visit('GET', '/clearing/account');
    delete process.env.CHORUS_SIGNIN_URL;
    expect(r.status).toBe(302);
    expect(r.location).toBe(`https://door.example/_auth/login?next=${encodeURIComponent('https://team.example.com/clearing/account')}`);
  });

  test('every same-mount link on the signed-in room answers (no 404)', async () => {
    const cookie = gate.guardCookie(gate.ALLOWED);
    const room = await gate.visit('GET', '/clearing/', cookie);
    const gets = rootLinks(room.body).filter((u) => !u.includes('/api/') && !u.includes('/socket.io'));
    expect(gets.length).toBeGreaterThan(0);
    const missing: string[] = [];
    for (const u of gets) {
      const r = await gate.visit('GET', u, cookie);
      if (r.status === 404) missing.push(u);
    }
    expect(missing).toEqual([]);
  });

  test('redirects stay inside the mount: naming yourself goes back to the room', async () => {
    const r = await gate.visit('POST', '/clearing/set-name', gate.guardCookie(gate.ALLOWED));
    expect(r.status).toBe(302);
    expect(r.location).toBe('/clearing/');
  });

  // NEGATIVE PROOF: the walk can tell a leaving link from a kept one
  test('NEGATIVE PROOF — a bare /account link and a bare redirect are caught', () => {
    expect(leaving(rootLinks('<a href="/account">Account</a><a href="/clearing/x">x</a>'))).toEqual(['/account']);
    expect(leaving(rootLinks("<style>a{background:url('/clearing-bg.jpg')}</style>"))).toEqual(['/clearing-bg.jpg']);
    expect(leaving(['/clearing', '/clearing/', '/clearing?x=1'])).toEqual([]);
  });
});
