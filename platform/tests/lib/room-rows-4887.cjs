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
// Loki unreachable, no reply to look for) — never a pass.
// usage: node room-rows-4887.cjs <playwright-core dir> <room url> <role[|alt…]> …
//   'silas|wren|kade' means the newest reply from any of them.
const [, , pw, url, ...need] = process.argv;
const { chromium } = require(pw);
const { contentHash } = require('../../chorus-sdk/reply/hash');
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

async function roomRecord() {
  if (process.env.ROOM_RECORD_FILE) return JSON.parse(require('fs').readFileSync(process.env.ROOM_RECORD_FILE, 'utf8'));
  const r = await fetch(new URL('api/messages', url), { signal: AbortSignal.timeout(10000) });
  if (!r.ok) throw new Error(`room record ${r.status}`);
  return r.json();
}

(async () => {
  const want = [];
  if (process.env.WANT_4887) {
    for (const w of process.env.WANT_4887.split(',')) { const [role, hash] = w.split(':'); want.push({ role, hash }); }
  } else {
    for (const n of need) want.push(await latestEmitted(n.split('|')));
  }
  const record = await roomRecord();
  for (const w of want) {
    const row = record.find((m) => m.from === w.role && contentHash(m.text || '') === w.hash);
    w.id = row ? row.id : null;
  }
  const b = await chromium.launch({
    executablePath: '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
    headless: true,
    args: ['--use-mock-keychain', '--password-store=basic'],
  });
  try {
    const p = await b.newPage();
    const errs = [];
    p.on('pageerror', (e) => errs.push(e.message));
    await p.goto(url, { waitUntil: 'load', timeout: 20000 });
    await p.waitForFunction('document.querySelectorAll("#messages-stream [data-from]").length > 0', { timeout: 20000 });
    const rows = await p.$$eval('#messages-stream [data-from]', (els) => els.map((e) => ({ id: e.id, from: e.dataset.from })));
    const seen = new Set(rows.map((r) => `${r.from}:${r.id}`));
    // not in the room's record at all, or in it but not on the page: both are missing
    const missing = want.filter((w) => !w.id || !seen.has(`${w.role}:msg-${w.id}`));
    console.log(JSON.stringify({ url, rows: rows.length, want, missing, errs }));
    process.exit(missing.length || errs.length ? 1 : 0);
  } finally {
    await b.close();
  }
})().catch((e) => { console.error(`could not measure: ${e.message}`); process.exit(2); });
