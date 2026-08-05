/**
 * Journey crawl — #3761's negative proof.
 *
 * AC: "a crawl of the integrated surfaces finds zero dead-end pages (no page
 * without an onward link) and zero orphan pages (no page unreachable from /)".
 *
 * BFS from / following RENDERED anchors (the family pages build their links
 * client-side from owl-api, so a static-HTML crawl would see empty shells —
 * Playwright renders first, then we read the DOM). Scope = the integrated
 * journey surfaces; parametrized pages (product.html?p=…, /domains/<d>) are
 * visited up to a per-pattern sample cap so the crawl stays minutes, not hours.
 *
 * Two failure classes, both loud, both name the page:
 *   DEAD-END — a visited in-scope page whose rendered DOM has no onward
 *              same-origin link (other than itself)
 *   ORPHAN   — a REQUIRED_RUNG never reached from /
 *
 * --fixture mode (the negative proof, per the no-gate-without-a-negative-proof
 * contract): serves a 3-page fixture containing one dead-end and one orphan,
 * runs the same crawl logic, and exits 0 ONLY if the crawl detects BOTH.
 * A crawl that cannot fail on a violated journey must not gate anything.
 *
 * Run (rides the gathering repo's node_modules, same as scrape-chorus-sitemap):
 *   NODE_PATH=../jeff-bridwell-personal-site/node_modules node platform/scripts/crawl-journey.cjs [--fixture]
 */

const { chromium } = require('playwright');
const http = require('http');

const BASE = process.env.CHORUS_UI_BASE || 'http://localhost:3340';
const FIXTURE_MODE = process.argv.includes('--fixture');

// Every rung of the ladder must be reachable from / by clicking only forward.
const REQUIRED_RUNGS = [
  '/',
  '/athena/value-stream.html',
  '/athena/products.html',
  '/athena/domains.html',
  '/athena/model.html',
  '/domains',
];

// What counts as "in scope" to VISIT (BFS expands only inside this set; links
// leaving the set are recorded as exits, not followed). Pattern → sample cap.
const SCOPE = [
  { re: /^\/$/, cap: 1 },
  { re: /^\/athena\/(value-stream|products|domains|model|tree)\.html$/, cap: 1 },
  { re: /^\/athena\/product\.html\?p=[^&]+$/, cap: 3 },
  { re: /^\/athena\/domain\.html\?d=[^&]+$/, cap: 3 },
  { re: /^\/domains$/, cap: 1 },
  { re: /^\/domains\/[^/]+$/, cap: 3 },
  { re: /^\/domains\/[^/]+\/[^/]+$/, cap: 2 },
];

function inScope(pathq, counts) {
  for (let i = 0; i < SCOPE.length; i++) {
    if (SCOPE[i].re.test(pathq)) {
      counts[i] = counts[i] || 0;
      if (counts[i] >= SCOPE[i].cap) return false;
      counts[i]++;
      return true;
    }
  }
  return false;
}

async function crawl(base, scope, required) {
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
  const counts = {};
  const visited = new Map(); // pathq → { outLinks: n }
  const queue = ['/'];
  const seen = new Set(['/']);
  const deadEnds = [];

  while (queue.length) {
    const pathq = queue.shift();
    const url = base + pathq;
    try {
      const resp = await page.goto(url, { waitUntil: 'networkidle', timeout: 20000 });
      if (!resp || resp.status() !== 200) {
        deadEnds.push(`${pathq} — HTTP ${resp ? resp.status() : 'no response'}`);
        visited.set(pathq, { outLinks: 0 });
        continue;
      }
      await page.waitForTimeout(600); // client renders settle
      const hrefs = await page.$$eval('a[href]', as => as.map(a => a.getAttribute('href')));
      const outs = new Set();
      for (const h of hrefs) {
        if (!h || h.startsWith('#') || h.startsWith('mailto:')) continue;
        let u;
        try { u = new URL(h, url); } catch { continue; }
        if (u.origin !== new URL(base).origin) continue; // cross-service (clearing :3470 etc.) = onward, not followed
        const pq = u.pathname + u.search;
        if (pq === pathq) continue;
        outs.add(pq);
        if (!seen.has(pq) && inScope(pq, counts)) { seen.add(pq); queue.push(pq); }
      }
      // any same-origin link that leaves the page counts as onward — including
      // exits from the scope set (a link into /loom/ is a continuing journey)
      const onward = outs.size + hrefs.filter(h => { try { return new URL(h, url).origin !== new URL(base).origin; } catch { return false; } }).length;
      visited.set(pathq, { outLinks: onward });
      if (onward === 0) deadEnds.push(`${pathq} — 0 onward links in the rendered DOM`);
    } catch (e) {
      deadEnds.push(`${pathq} — ${e.message.split('\n')[0]}`);
      visited.set(pathq, { outLinks: 0 });
    }
  }
  await browser.close();

  const reached = new Set(visited.keys());
  for (const pq of seen) reached.add(pq); // discovered = reachable even if capped out of a visit
  const orphans = required.filter(r => !reached.has(r));
  return { visited, deadEnds, orphans };
}

async function runFixture() {
  // a.html → links to b.html; b.html = DEAD-END (no links); c.html = ORPHAN
  const pages = {
    '/': '<html><body><h1>fixture root</h1><a href="/b.html">to b</a></body></html>',
    '/b.html': '<html><body><p>terminal page, no links — the dead-end</p></body></html>',
    '/c.html': '<html><body><a href="/">back</a></body></html>',
  };
  const srv = http.createServer((req, res) => {
    const body = pages[req.url.split('?')[0]];
    if (body) { res.writeHead(200, { 'content-type': 'text/html' }); res.end(body); }
    else { res.writeHead(404); res.end('nope'); }
  });
  await new Promise(r => srv.listen(0, r));
  const base = `http://localhost:${srv.address().port}`;
  const scope = [{ re: /^\/$/, cap: 1 }, { re: /^\/b\.html$/, cap: 1 }, { re: /^\/c\.html$/, cap: 1 }];
  const saved = SCOPE.splice(0, SCOPE.length, ...scope);
  const { deadEnds, orphans } = await crawl(base, scope, ['/', '/b.html', '/c.html']);
  SCOPE.splice(0, SCOPE.length, ...saved);
  srv.close();

  const caughtDeadEnd = deadEnds.some(d => d.startsWith('/b.html'));
  const caughtOrphan = orphans.includes('/c.html');
  console.log(`fixture: dead-end detected=${caughtDeadEnd} orphan detected=${caughtOrphan}`);
  if (caughtDeadEnd && caughtOrphan) {
    console.log('NEGATIVE PROOF OK — the crawl fails on a violated journey.');
    process.exit(0);
  }
  console.error('NEGATIVE PROOF FAILED — the crawl cannot detect the violation it exists to catch. It must not gate.');
  process.exit(1);
}

(async () => {
  if (FIXTURE_MODE) return runFixture();
  const { visited, deadEnds, orphans } = await crawl(BASE, SCOPE, REQUIRED_RUNGS);
  console.log(`journey crawl — ${visited.size} pages visited from ${BASE}/`);
  for (const [pq, v] of visited) console.log(`  ${pq} — ${v.outLinks} onward`);
  if (deadEnds.length) { console.error(`\nDEAD-ENDS (${deadEnds.length}):`); deadEnds.forEach(d => console.error('  ✗ ' + d)); }
  if (orphans.length) { console.error(`\nORPHANS (${orphans.length} required rungs unreached from /):`); orphans.forEach(o => console.error('  ✗ ' + o)); }
  if (deadEnds.length || orphans.length) process.exit(1);
  console.log('\nZERO dead-ends, ZERO orphans — the journey continues from everywhere.');
})();
