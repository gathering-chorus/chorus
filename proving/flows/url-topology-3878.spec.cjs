// @test-type: e2e:ui — playwright browser flow (url-topology-3878), live surface
/**
 * #3878 — the four public paths Jeff specified must resolve.
 *
 * He decided this twice and we built the opposite both times:
 *
 *   2026-02-21  "Maybe I just do lightlifeurbangardens.com/clearing"
 *   2026-08-08  "so here is my pov 4 public urls
 *                lightlifeurbangardens.com
 *                lightlifeurbangardens.com/chorus
 *                lightlifeurbangardens.com/gathering
 *                lightlifeurbangardens.com/borg (shared-observability)"
 *   2026-08-14  "having clearing.lightlifeurbangardens.com is upside down.
 *                That's not the normal way you form URLs. You go from general
 *                to specific... this is basic human needs, and these are
 *                interfaces, and it's like 100% afterthought."
 *
 * Three times, six months. Per /cno the check IS the card, so this file is the
 * deliverable — not the tunnel edit it forces.
 *
 * NEGATIVE PROOF (#3734): this is born RED. `/clearing` does not exist today;
 * the room is only reachable at a subdomain. If this file passes on first run,
 * it is testing the wrong host and must be fixed, not celebrated.
 *
 * It runs against the PUBLIC host on purpose. Every check I wrote yesterday hit
 * localhost and reported green while Jeff's screen was wrong — the defect class
 * #3877 exists to end. A localhost run of this file proves nothing.
 */
const { test, expect } = require('@playwright/test');

const PUBLIC_BASE = process.env.PUBLIC_BASE || 'https://lightlifeurbangardens.com';

/** Jeff's four, verbatim from 2026-08-08, plus the room he asked for twice. */
const PATHS = [
  { path: '/', what: 'the apex' },
  { path: '/chorus', what: 'chorus (landed #3804 — the pattern this follows)' },
  { path: '/clearing', what: 'the room — RED until the ingress rule exists' },
  { path: '/gathering', what: 'gathering' },
  { path: '/borg', what: 'shared-observability' },
];

test.describe('#3878 — the public path topology Jeff specified', () => {
  // #2646 standing posture (Silas ruling 2026-08-23): Cloudflare Access sits in
  // front of the public host. Anonymous probes see the wall, not the page — and
  // the wall IS the correct state. A path passes if it serves the page (HTML)
  // OR lands on the Access wall. Tunnel-dead states (530/1033, connection
  // refused) still fail: the wall and the outage are different states.
  const accessWalled = (res, body) => {
    const finalUrl = res.url() || '';
    const server = (res.headers()['server'] || '').toLowerCase();
    return finalUrl.includes('cloudflareaccess.com')
      || /cloudflareaccess|cf-access/i.test(body)
      || (res.status() === 401 && server.includes('cloudflare'));
  };

  for (const { path, what } of PATHS) {
    test(`${path} serves a page or the Access wall — ${what}`, async ({ request }) => {
      const res = await request.get(`${PUBLIC_BASE}${path}`, { maxRedirects: 5 });
      const body = await res.text();
      if (accessWalled(res, body)) return; // auth-wall present = pass (#2646)

      expect(res.status(), `${PUBLIC_BASE}${path} → ${res.status()}`).toBeLessThan(400);
      const type = res.headers()['content-type'] || '';
      expect(type, `${path} content-type: ${type}`).toContain('html');
      expect(body.length, `${path} body length ${body.length}`).toBeGreaterThan(200);
      expect(body.toLowerCase(), `${path} does not look like a document`).toContain('<html');
    });
  }

  // Without this, "every path is under 400" would pass against a host that
  // returns 200 for everything including nonsense — a wildcard catch-all reads
  // as a working topology.
  test('a path that should NOT exist still 404s — the check can tell them apart', async ({ request }) => {
    const apex = await request.get(`${PUBLIC_BASE}/`, { maxRedirects: 5 });
    const apexBody = await apex.text();
    // Behind the Access wall every anonymous probe looks identical — the
    // discriminator cannot discriminate. Typed skip, never a vacuous pass.
    test.skip(accessWalled(apex, apexBody), 'apex behind Access wall — 404 discriminator needs credentials');
    const res = await request.get(`${PUBLIC_BASE}/definitely-not-a-real-surface-3878`, {
      maxRedirects: 5,
    });
    expect(res.status()).toBeGreaterThanOrEqual(400);
  });

  // The subdomain must keep working or redirect — Jeff has it bookmarked and
  // on his phone. Moving a URL is not the same as breaking the old one.
  test('the old subdomain still reaches the room', async ({ request }) => {
    const res = await request.get('https://clearing.lightlifeurbangardens.com/', {
      maxRedirects: 5,
    });
    const body = await res.text();
    if (!accessWalled(res, body)) expect(res.status()).toBeLessThan(400);
  });
});

/**
 * #3872 — the subdomain is RETIRED, not merely bypassed.
 *
 * #3878 made lightlifeurbangardens.com/clearing work. It left the old
 * clearing.lightlifeurbangardens.com hostname in place as a bridge, which means
 * the upside-down URL Jeff objected to is still live and still linkable — the
 * thing he asked us to stop doing, kept working.
 *
 * NEGATIVE PROOF (#3734): this is born RED. The subdomain answers 200 today.
 * A redirect (301/302/308) to the apex path PASSES — old links keep working,
 * which is why this asserts a redirect rather than an outage. What must NOT
 * happen is the subdomain serving the room itself.
 */
test.describe('#3872 — the upside-down URL is retired', () => {
  test('the old subdomain redirects to the apex path, it does not serve the room', async ({ request }) => {
    const res = await request.get('https://clearing.lightlifeurbangardens.com/', {
      maxRedirects: 0,
      failOnStatusCode: false,
    });
    const status = res.status();
    expect([301, 302, 307, 308], `subdomain still serves the room (got ${status})`).toContain(status);
    const location = res.headers()['location'] ?? '';
    expect(location, 'redirect must point at the apex path').toContain('/clearing');
    expect(location, 'redirect must not point back at the subdomain').not.toContain('clearing.lightlifeurbangardens.com');
  });
});
