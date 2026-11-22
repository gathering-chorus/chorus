// @domain: messages
// @card: #4887 · owner: silas
// AC1: load the PUBLIC room the way Jeff does (headless Chrome on the served page)
// and check that each named role's LATEST reply — its newest reply.emitted on the
// spine, read from Loki, at least a minute old — is a row the page rendered. The
// reply is found in the room's record by role + content hash; the page must hold
// that row's element (#msg-<id>, same role). Any old row of the role does not count.
// WANT_4887="role:hash,…" and ROOM_RECORD_FILE replace Loki and the record (the
// negative proof's fixture).
// Exit 0 only when every role's latest reply rendered and the page threw nothing;
// 1 names the replies that did not render; 2 means it could not measure (page or
// Loki unreachable, no reply to look for) — never a pass; 3 means a role's latest
// reply is older than the page's oldest row (the page scrolled past it): unmeasured,
// the same call the gap watch makes, never a red and never a pass.
// usage: node room-rows-4887.cjs <playwright-core dir> <room url> <role[|alt…]> …
//   'silas|wren|kade' means the newest reply from any of them.
const [, , pw, url, ...need] = process.argv;
const { chromium } = require(pw);
const { contentHash } = require('../../chorus-sdk/reply/hash');
const auth = require('../../../proving/flows/_auth.cjs');
const LOKI = process.env.LOKI_URL || 'http://localhost:3102';

async function latestEmitted(roles) {
  const now = Date.now();
  const q = new URLSearchParams({
    query: `{job="platform-chorus"} |= "reply.emitted" | json | event="reply.emitted" | role=~"${roles.join('|')}"`,
    start: String((now - 24 * 3600e3) * 1e6), end: String((now - 60e3) * 1e6), limit: '1', direction: 'backward',
  });
  const r = await fetch(`${LOKI}/loki/api/v1/query_range?${q}`, { signal: AbortSignal.timeout(10000) });
  if (!r.ok) throw new Error(`loki ${r.status}`);
  const lines = (await r.json()).data.result.flatMap((s) => s.values).sort((a, b) => (a[0] < b[0] ? 1 : -1));
  if (!lines.length) throw new Error(`no reply.emitted from ${roles.join('|')} in 24h`);
  const ev = JSON.parse(lines[0][1]);
  return { role: ev.role, hash: ev.hash, at: ev.timestamp };
}

// #4915 — the public room needs a sign-in (401 "not-signed-in" since #4580/#5015): read
// its record from the signed-in page, as Jeff's browser does, never anonymously.
async function roomRecord(page) {
  if (process.env.ROOM_RECORD_FILE) return JSON.parse(require('fs').readFileSync(process.env.ROOM_RECORD_FILE, 'utf8'));
  const got = await page.evaluate(async () => {
    const r = await fetch('api/messages', { signal: AbortSignal.timeout(10000) });
    return { ok: r.ok, status: r.status, body: r.ok ? await r.json() : null };
  });
  if (!got.ok) throw new Error(`room record ${got.status}`);
  return got.body;
}

(async () => {
  const want = [];
  if (process.env.WANT_4887) {
    for (const w of process.env.WANT_4887.split(',')) { const [role, hash] = w.split(':'); want.push({ role, hash }); }
  } else {
    for (const n of need) want.push(await latestEmitted(n.split('|')));
  }
  const b = await chromium.launch({
    executablePath: '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
    headless: true,
    args: ['--use-mock-keychain', '--password-store=basic'],
  });
  const p = await b.newPage();
  let code = 2;
  let verdict = null;
  let signOut = 'not signed in';
  try {
    const errs = [];
    if (!process.env.ROOM_RECORD_FILE) {
      // no creds on this account (a role's card run): unmeasured (3), the _auth.cjs rule —
      // never a pass and never a red; the nightly runs as the account that holds them
      if (!auth.haveCreds) { console.log('unmeasured: no flow creds on this account (~/.chorus/flow-creds.env); the room needs a sign-in'); await b.close(); process.exit(3); }
      await auth.gotoSignedIn(p, url);
    }
    // #4915 review — listen only on the room: an error on the sign-in or consent page is
    // not the room's
    p.on('pageerror', (e) => errs.push(e.message));
    await p.goto(url, { waitUntil: 'load', timeout: 20000 });
    const record = await roomRecord(p);
    for (const w of want) {
      const row = record.find((m) => m.from === w.role && contentHash(m.text || '') === w.hash);
      w.id = row ? row.id : null;
      w.ts = row ? row.ts : (w.at || null);
    }
    await p.waitForFunction('document.querySelectorAll("#messages-stream [data-from]").length > 0', { timeout: 20000 });
    const rows = await p.$$eval('#messages-stream [data-from]', (els) => els.map((e) => ({ id: e.id, from: e.dataset.from, ts: e.dataset.ts || '' })));
    const seen = new Set(rows.map((r) => `${r.from}:${r.id}`));
    const oldest = Math.min(...rows.map((r) => Date.parse(r.ts)).filter((t) => !Number.isNaN(t)));
    const absent = want.filter((w) => !w.id || !seen.has(`${w.role}:msg-${w.id}`));
    // absent AND older than the page's oldest row: the page scrolled past it — unmeasured
    const scrolled = absent.filter((w) => w.ts && Date.parse(w.ts) < oldest);
    const missing = absent.filter((w) => !scrolled.includes(w));
    verdict = { url, rows: rows.length, want, missing, scrolled, errs };
    code = missing.length || errs.length ? 1 : scrolled.length ? 3 : 0;
  } finally {
    // #5026 — a probe that signed in signs out, or its Session row stays open. The verdict
    // exits AFTER this (#4915 review: an exit inside the try skipped the sign-out)
    // A failed sign-out does not change the verdict (the _auth.cjs rule: the door ends the
    // row at the next sign-in, #4966), but it is printed in the verdict line, never only on
    // stderr (#4915 review: a leak nobody sees)
    signOut = await auth.signOutIfSignedIn(p, new URL(url).origin)
      .then((did) => (did ? 'signed out' : 'not signed in'), (e) => `FAILED: ${e.message}`);
    if (signOut.startsWith('FAILED')) console.error(`sign-out ${signOut} — the Session row stays open until the next sign-in`);
    await b.close();
  }
  if (verdict) console.log(JSON.stringify({ ...verdict, signOut }));
  process.exit(code);
})().catch((e) => { console.error(`could not measure: ${e.message}`); process.exit(2); });
