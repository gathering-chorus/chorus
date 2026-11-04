// @domain: messages
// @card: #4887 · owner: silas
// AC1: load the room the way Jeff does — headless Chrome on the served page — and
// say which roles' rows it actually rendered. Exit 0 only if every required role
// has at least one rendered row and the page threw nothing; 1 names the missing
// roles; 2 means the page could not be loaded at all (never a pass).
// usage: node room-rows-4887.cjs <playwright-core dir> <room url> <role[|alt…]> …
//   a role argument with '|' is satisfied by any one of its names (a Claude role).
const [, , pw, url, ...need] = process.argv;
const { chromium } = require(pw);
(async () => {
  const b = await chromium.launch({
    executablePath: '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
    headless: true,
    args: ['--use-mock-keychain', '--password-store=basic'],
  });
  try {
    const p = await b.newPage();
    const errs = [];
    p.on('pageerror', (e) => errs.push(e.message));
    await p.goto(url, { waitUntil: 'load', timeout: 15000 });
    await p.waitForFunction('document.querySelectorAll("#messages-stream [data-from]").length > 0', { timeout: 15000 });
    const rows = await p.$$eval('#messages-stream [data-from]', (els) => els.map((e) => e.dataset.from));
    const from = {};
    for (const r of rows) from[r] = (from[r] || 0) + 1;
    const missing = need.filter((n) => !n.split('|').some((r) => from[r]));
    console.log(JSON.stringify({ rows: rows.length, from, missing, errs }));
    process.exit(missing.length || errs.length ? 1 : 0);
  } finally {
    await b.close();
  }
})().catch((e) => { console.error(`room did not load: ${e.message}`); process.exit(2); });
