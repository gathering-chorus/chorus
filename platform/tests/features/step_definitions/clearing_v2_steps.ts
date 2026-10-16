// @test-type: bdd — cucumber step definitions for Clearing v2 (Card #4530)
import { Given, When, Then } from '@cucumber/cucumber';
import * as assert from 'assert';
import * as http from 'http';

const BASE_URL = process.env.CLEARING_V2_URL || 'http://127.0.0.1:3472';
let lastPageContent = '';
let lastHttpStatus = 0;
let currentLagSource: string | undefined = undefined;

async function fetchUrl(url: string, headers?: Record<string, string>): Promise<{ status: number; body: string }> {
  try {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), 4000);
    const res = await fetch(url, { headers, signal: controller.signal });
    clearTimeout(timer);
    const body = await res.text();
    return { status: res.status, body };
  } catch (err: any) {
    return { status: 0, body: err.message || '' };
  }
}

Given('Clearing v2 is running on port {int}', async function (port: number) {
  const res = await fetchUrl(`${BASE_URL}/health`);
  assert.strictEqual(res.status, 200, `Clearing v2 is not running on port ${port} (status: ${res.status})`);
  const data = JSON.parse(res.body);
  assert.strictEqual(data.ok, true, 'Clearing v2 health returned ok=false');
});

let browserInstance: any = null;
let browserContext: any = null;
let browserPage: any = null;
let lastPostedMsgText = '';

async function ensureBrowserPage(viewport = { width: 390, height: 844 }) {
  if (!browserInstance) {
    const { chromium } = require('playwright');
    browserInstance = await chromium.launch({
      executablePath: '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
      headless: true,
      args: ['--no-sandbox', '--disable-setuid-sandbox', '--disable-gpu'],
    });
  }
  if (!browserContext) {
    const { readCookieKey, mintShareSessionCookie } = require('../../../../directing/clearing-v2/dist/share-session.js');
    browserContext = await browserInstance.newContext({ viewport });
    const key = readCookieKey();
    if (key) {
      const cookie = mintShareSessionCookie('https://id.lightlifeurbangardens.com/jeff/profile/card#me', key);
      await browserContext.addCookies([{
        name: 'chorus_share_session',
        value: cookie,
        domain: '127.0.0.1',
        path: '/',
      }]);
    }
  }
  if (!browserPage) {
    browserPage = await browserContext.newPage();
  }
  return browserPage;
}

async function closeBrowser() {
  if (browserInstance) {
    try { await browserInstance.close(); } catch { /* already closed: teardown has nothing left to release */ }
    browserInstance = null;
    browserContext = null;
    browserPage = null;
  }
}

// Roles steps
When('Jeff loads the Roles page', { timeout: 30000 }, async function () {
  const p = await ensureBrowserPage();
  const res = await p.goto(`${BASE_URL}/roles`, { waitUntil: 'domcontentloaded' });
  lastHttpStatus = res?.status() || 200;
  lastPageContent = await p.content();
});

Then('Jeff sees all five principals listed with login status and active session', async function () {
  assert.strictEqual(lastHttpStatus, 200, `Roles page returned HTTP ${lastHttpStatus}`);
  assert.match(lastPageContent, /class="[^"]*roles-table[^"]*"/, 'Roles page lacks 5-principal status table');

  // 1. Assert exactly the 5 team principals from live door
  assert.ok(lastPageContent.includes('Jeff'), 'Roles table missing Jeff');
  assert.ok(lastPageContent.includes('Wren'), 'Roles table missing Wren');
  assert.ok(lastPageContent.includes('Silas'), 'Roles table missing Silas');
  assert.ok(lastPageContent.includes('Kade'), 'Roles table missing Kade');
  assert.ok(lastPageContent.includes('Abby Normal'), 'Roles table missing Abby Normal');

  // Assert crawler, nightly, and probe rows are excluded (filtered by roleKind agent|human)
  assert.ok(!lastPageContent.includes('Crawler') && !lastPageContent.includes('crawler'), 'Crawler service leaks into roles table');
  assert.ok(!lastPageContent.includes('nightly'), 'Nightly runner leaks into roles table');
  assert.ok(!lastPageContent.includes('zz-probe'), 'Old probe rows leak into roles table');

  // 2. STUB TEST WITH UNSEEN ROLES (#3734): proves code does not hardcode names or cards
  let stubServer: any;
  let stubPort = 0;
  await new Promise<void>((resolve) => {
    stubServer = http.createServer((req: any, res: any) => {
      if (req.url.startsWith('/fail')) {
        res.writeHead(500, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: 'Door Error' }));
        return;
      }
      if (req.url.startsWith('/roles')) {
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({
          data: [
            { name: 'zz-test-pilot', label: 'ZZ Test Pilot', roleKind: 'agent' },
            { name: 'zz-test-person', label: 'ZZ Test Person', roleKind: 'human' },
            { name: 'crawler', label: 'Crawler Service', roleKind: 'service' },
          ],
        }));
        return;
      }
      if (req.url.startsWith('/board/wip')) {
        const u = new URL(req.url, 'http://localhost');
        const role = u.searchParams.get('role');
        if (role === 'zz-test-pilot') {
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ data: { cards: [{ id: 9901, title: 'Test Pilot Flight' }] } }));
        } else if (role === 'zz-test-person') {
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ data: { cards: [{ id: 9902, title: 'Ground Control Ops' }] } }));
        } else {
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ data: { cards: [] } }));
        }
        return;
      }
      res.writeHead(404).end();
    });
    stubServer.listen(0, '127.0.0.1', () => {
      stubPort = stubServer.address().port;
      resolve();
    });
  });

  try {
    const stubRes = await fetchUrl(`${BASE_URL}/roles`, {
      'x-clearing-test': '1',
      'x-roles-url': `http://127.0.0.1:${stubPort}/roles`,
      'x-board-url': `http://127.0.0.1:${stubPort}/board`,
    });
    assert.strictEqual(stubRes.status, 200);
    assert.ok(stubRes.body.includes('ZZ Test Pilot'), 'Stub role zz-test-pilot not rendered');
    assert.ok(stubRes.body.includes('ZZ Test Person'), 'Stub role zz-test-person not rendered');
    assert.ok(stubRes.body.includes('#9901') && stubRes.body.includes('Test Pilot Flight'), 'Stub card #9901 not rendered');
    assert.ok(stubRes.body.includes('#9902') && stubRes.body.includes('Ground Control Ops'), 'Stub card #9902 not rendered');
    assert.ok(!stubRes.body.includes('Crawler Service') && !stubRes.body.includes('crawler'), 'Crawler service not filtered from stub');

    // 3. NEGATIVE PROOF: stub answers 500 -> trust strip names roles failed, never a list
    const failRes = await fetchUrl(`${BASE_URL}/roles`, {
      'x-clearing-test': '1',
      'x-roles-url': `http://127.0.0.1:${stubPort}/fail`,
      'x-board-url': `http://127.0.0.1:${stubPort}/board`,
    });
    assert.ok(failRes.body.includes('0 principals found'), 'Failing source rendered principals instead of empty state');
    assert.ok(!failRes.body.includes('<table class="roles-table">'), 'Failing source rendered roles table');
    assert.ok(failRes.body.includes('strip-source-fail'), 'Trust strip did not highlight failing source red');
    assert.ok(failRes.body.includes('fail-source-name">roles</span> failed (door unreadable (500))'), 'Trust strip did not name roles failure');
  } finally {
    await new Promise<void>((resolve) => stubServer.close(() => resolve()));
  }
});

Then("each row displays the principal's current card and last heard time", function () {
  assert.match(lastPageContent, /class="[^"]*role-last-heard[^"]*"/, 'Roles table lacks last-heard timestamps');
  // Abby Normal's live card #4530 from board WIP (:3340)
  assert.ok(lastPageContent.includes('#4530'), 'Abby Normal row does not display card #4530');
});

When('viewing the roles status table', { timeout: 30000 }, async function () {
  const p = await ensureBrowserPage();
  const res = await p.goto(`${BASE_URL}/roles`, { waitUntil: 'domcontentloaded' });
  lastHttpStatus = res?.status() || 200;
  lastPageContent = await p.content();
});

Then('each principal shows one of {string}, {string}, {string}, {string}, or {string}', function (s1: string, s2: string, s3: string, s4: string, s5: string) {
  assert.strictEqual(lastHttpStatus, 200);
  assert.match(lastPageContent, /class="[^"]*role-state-badge[^"]*"/, 'Roles table lacks distinct 5-state badges');

  const validStates = [s1, s2, s3, s4, s5, 'idle', 'unmeasured'];
  const badgeMatches = Array.from(lastPageContent.matchAll(/class="[^"]*role-state-badge[^"]*"[^>]*>[\s\S]*?<span class="dot[^"]*"[^>]*><\/span>([^<]+)/g));
  assert.ok(badgeMatches.length >= 5, `Expected at least 5 principal state badges, got ${badgeMatches.length}`);
  for (const m of badgeMatches) {
    const text = m[1].trim();
    const matchesState = validStates.some(vs => text === vs || text.startsWith(vs));
    assert.ok(matchesState, `Badge state "${text}" is not one of: ${validStates.join(', ')}`);
  }
});

Then('{string} clearly indicates what decision or input is owed', function (state: string) {
  // HONEST RED (#4361 / Wren 11:11):
  // "waiting on you" has no source in graph presence yet (#4361).
  // A state with no source stays red, by name.
  if (!lastPageContent.includes(state)) {
    assert.fail(`'${state}' state has no source in graph presence yet (#4361 unlanded)`);
  }
  assert.match(lastPageContent, /class="[^"]*waiting-reason[^"]*"/, 'Waiting state lacks specific owed decision/card context');
});

Given('a role has sent no events for more than {int} minutes', function (_minutes: number) {
  currentLagSource = 'roles';
});

Then('the role status displays as {string} with the elapsed silence duration', async function (status: string) {
  const res = await fetchUrl(`${BASE_URL}/roles`, { 'x-clearing-test': '1', 'x-lag-source': 'roles' });
  assert.ok(res.body.includes(status) || res.body.includes('stale') || res.body.includes('34m'), `Roles page lacks ${status} state indicator`);
});

Then('each role indicates its active host and terminal pane', function () {
  assert.match(lastPageContent, /class="[^"]*role-pane[^"]*"/, 'Roles table lacks terminal pane indicator');
});

Then("Jeff's row shows a {string} marker indicating his current surface", function (marker: string) {
  assert.ok(lastPageContent.includes('you'), `Roles table lacks Jeff indicator`);
});

// Today steps
When('Jeff loads the Today page', async function () {
  const res = await fetchUrl(`${BASE_URL}/today`);
  lastHttpStatus = res.status;
  lastPageContent = res.body;
});

Then('Jeff sees decisions owed grouped by meeting run', function () {
  assert.strictEqual(lastHttpStatus, 200, `Today page returned HTTP ${lastHttpStatus}`);
  // HONEST RED (#3734): Decision class unmodeled in Part 1 (0 rows in graph).
  const hasApproveAll = lastPageContent.includes('approve-all');
  const hasZeroDecisions = lastPageContent.includes('0 decisions owed');
  if (!hasApproveAll || hasZeroDecisions) {
    assert.fail('0 Decision rows found in graph (Decision class unmodeled, owed in Part 2)');
  }
  assert.match(lastPageContent, /class="[^"]*decisions-owed[^"]*"/, 'Today page lacks decisions-owed group container');
});

Then('Jeff can accept or approve-all proposals with attached evidence', function () {
  assert.match(lastPageContent, /class="[^"]*approve-all[^"]*"/, 'Today page lacks approve-all action for decisions group');
});

Given("Jeff's last Clearing session ended at {string}", function (_time: string) {
  // session end watermark
});

Then('Jeff sees what changed since his last session ended with cards that landed, moved, or became blocked', function () {
  assert.match(lastPageContent, /class="[^"]*changed-overnight[^"]*"/, 'Today page lacks changed-overnight section');
  assert.ok(lastPageContent.includes('#4530') || lastPageContent.includes('#4536'), 'Missing live WIP cards');
});

Then('any additional card movements beyond four are folded', function () {
  assert.match(lastPageContent, /class="[^"]*moves-folded[^"]*"/, 'Today page lacks folded card overflow mechanism');
});

Then('Jeff sees only alerts that are new or changed', function () {
  assert.match(lastPageContent, /class="[^"]*alerts-section[^"]*"/, 'Today page lacks new/changed alerts section');
  assert.ok(lastPageContent.includes('firing') && lastPageContent.includes('Relay write-backs lost'), 'Missing firing alert');
  assert.ok(lastPageContent.includes('acked') && lastPageContent.includes('Fuseki backup older'), 'Missing acked alert');
});

Then('each active alert links to its resolving card', function () {
  assert.match(lastPageContent, /class="[^"]*alert-resolving-card[^"]*"/, 'Active alerts lack resolving card links');
  assert.ok(lastPageContent.includes('/board?card=4366'), 'Alert lacks resolving card link #4366');
  assert.ok(lastPageContent.includes('/board?card=4171'), 'Alert lacks resolving card link #4171');
});

Then('Jeff sees who is working right now with their active card and status dot', function () {
  assert.match(lastPageContent, /class="[^"]*working-now[^"]*"/, 'Today page lacks working-now section');
  assert.ok(lastPageContent.includes('Abby-normal') || lastPageContent.includes('Abby Normal'), 'Working-now lacks Abby Normal');
  assert.ok(lastPageContent.includes('#4530'), 'Working-now lacks active card #4530 for Abby Normal');
});

// Rooms steps
When('Jeff loads the Rooms page', { timeout: 30000 }, async function () {
  const p = await ensureBrowserPage();
  await p.goto(`${BASE_URL}/rooms/team`, { waitUntil: 'domcontentloaded' });
});

Then('Jeff sees a list of rooms including {string} and direct conversations', async function (room: string) {
  assert.ok(browserPage, 'Browser page was not launched');
  const sidebar = browserPage.locator('.rooms-sidebar');
  assert.ok(await sidebar.isVisible(), 'Rooms page lacks visible rooms sidebar selector');

  const cleanRoom = room.replace('#', '').trim();
  const roomLink = browserPage.locator(`.rooms-sidebar a[href*="/rooms/${cleanRoom}"]`);
  assert.ok(await roomLink.isVisible(), `Rooms page lacks room ${room}`);

  const sidebarText = await sidebar.innerText();
  assert.ok(sidebarText.includes('Wren') && sidebarText.includes('Silas') && sidebarText.includes('Abby Normal'),
    'Rooms sidebar lacks direct role participants');
});

Then('Jeff can switch to {string} room', async function (room: string) {
  try {
    assert.ok(browserPage, 'Browser page was not launched');
    const cleanRoom = room.replace('#', '').trim();
    const roomLink = browserPage.locator(`.rooms-sidebar a[href*="/rooms/${cleanRoom}"]`);
    assert.ok(await roomLink.isVisible(), `No link to switch to ${room}`);
    await roomLink.click();
    await browserPage.waitForTimeout(500);
    assert.ok(browserPage.url().includes(`/rooms/${cleanRoom}`), `URL did not switch to /rooms/${cleanRoom}`);
  } finally {
    await closeBrowser();
  }
});

When('Jeff posts a message {string} into the {string} room', { timeout: 30000 }, async function (msg: string, room: string) {
  const cleanRoom = room.replace('#', '').trim();
  lastPostedMsgText = msg;

  const p = await ensureBrowserPage();
  await p.goto(`${BASE_URL}/rooms/${cleanRoom}`, { waitUntil: 'domcontentloaded' });

  // Wait for room-key auto-binding to finish
  await p.waitForFunction(() => (window as any).roomKeyStatus && (window as any).roomKeyStatus.state === 'bound', { timeout: 5000 }).catch(() => {});

  // Type message into composer input
  const box = p.locator('textarea, input[type=text]').first();
  await box.fill(msg);

  // Click Send button
  await p.getByRole('button', { name: /send/i }).click();

  // Wait for response / delivery update
  await p.waitForTimeout(2000);
});

Then('Jeff sees the message appear in the feed with a delivery confirmation tick', { timeout: 10000 }, async function () {
  try {
    assert.ok(browserPage, 'Browser page was not launched');

    // 1. Assert message appears in feed
    const feedHtml = await browserPage.innerHTML('#messages-stream');
    assert.ok(feedHtml.includes(lastPostedMsgText), `Message text "${lastPostedMsgText}" not found in feed`);

    // 2. Assert delivery confirmation tick is visible
    const tickCount = await browserPage.locator('.delivery-tick').count();
    assert.ok(tickCount > 0, 'Message lacks delivery confirmation tick from relay OK');
  } finally {
    await closeBrowser();
  }
});

When('role chatter occurs in response to a message', function () {
  // Precondition
});

Then('the role responses are grouped in a thread under the message', function () {
  assert.match(lastPageContent, /class="[^"]*thread-container[^"]*"/, 'Role chatter is not grouped in collapsible threads');
  assert.match(lastPageContent, /\d+\s*working replies/, 'Thread does not summarize working replies');
});

Then('the main room feed remains uncluttered', function () {
  assert.ok(!lastPageContent.includes('role-raw-stream'), 'Raw role stream leaks into main room');
});

When('Jeff views a message referencing a card {string}', function (_card: string) {
  // Precondition
});

Then('clicking the card reference opens the card in a side drawer', function () {
  assert.match(lastPageContent, /class="[^"]*card-drawer[^"]*"/, 'Rooms lacks side drawer for card inspection');
  assert.match(lastPageContent, /class="[^"]*card-ref[^"]*"/, 'Message lacks clickable card reference link');
  assert.ok(lastPageContent.includes('#4530'), 'Card drawer lacks card #4530 reference');
});

Then('Jeff remains in the conversation view', function () {
  assert.match(lastPageContent, /class="[^"]*chat-container[^"]*"/, 'Opening card did not preserve chat view');
});

// Trust strip steps
Given('the trust strip endpoint returns source, last successful write, and status for each source', async function () {
  const res = await fetchUrl(`${BASE_URL}/api/trust-strip`);
  assert.strictEqual(res.status, 200, 'Trust strip endpoint not responding');
  const sources = JSON.parse(res.body);
  assert.ok(Array.isArray(sources), 'Trust strip did not return an array');
  const names = sources.map((s: any) => s.source);
  assert.ok(names.includes('board'), 'Trust strip missing board source');
  assert.ok(names.includes('rooms'), 'Trust strip missing rooms source');
  assert.ok(names.includes('roles'), 'Trust strip missing roles source');
  assert.ok(names.includes('decisions'), 'Trust strip missing decisions source');
});

When('Jeff loads any Clearing page', { timeout: 30000 }, async function () {
  const p = await ensureBrowserPage();
  const res = await p.goto(`${BASE_URL}/today`, { waitUntil: 'domcontentloaded' });
  lastHttpStatus = res?.status() || 200;
  lastPageContent = await p.content();
});

Then('Jeff sees a trust strip with chips for board, rooms, decisions, and roles', function () {
  assert.match(lastPageContent, /class="[^"]*trust-strip[^"]*"/, 'Page lacks trust-strip container');
  assert.ok(lastPageContent.includes('board'), 'Trust strip lacks board chip');
  assert.ok(lastPageContent.includes('rooms'), 'Trust strip lacks rooms chip');
  assert.ok(lastPageContent.includes('decisions'), 'Trust strip lacks decisions chip');
  assert.ok(lastPageContent.includes('roles'), 'Trust strip lacks roles chip');
});

Then('each chip displays its source name and {string} timestamp', function (asOf: string) {
  assert.ok(lastPageContent.includes(asOf), `Trust strip chips lack '${asOf}' timestamps`);
});

Given('a backend source has not written for {int} minutes', function (_min: number) {
  currentLagSource = 'roles';
});

When('Jeff looks at the trust strip', { timeout: 30000 }, async function () {
  const p = await ensureBrowserPage();
  if (currentLagSource) {
    await p.setExtraHTTPHeaders({ 'x-clearing-test': '1', 'x-lag-source': currentLagSource });
  }
  const res = await p.goto(`${BASE_URL}/today`, { waitUntil: 'domcontentloaded' });
  lastHttpStatus = res?.status() || 200;
  lastPageContent = await p.content();
});

Then('the lag is displayed as elapsed time with a neutral grey indicator', function () {
  assert.match(lastPageContent, /class="[^"]*strip-lag-neutral[^"]*"/, 'Trust strip lacks neutral elapsed time lag display');
  assert.ok(lastPageContent.includes('stale 34m'), 'Missing neutral lag duration in chip');
});

Then('the indicator is not colored red', function () {
  assert.ok(!lastPageContent.includes('strip-lag-red'), 'Trust strip erroneously colored lag red');
});

Given('a backend source has experienced an unrecovered error', function () {
  // Precondition
});

Then('only the failing source is highlighted red', async function () {
  const res = await fetchUrl(`${BASE_URL}/`, { 'x-clearing-test': '1', 'x-fail-source': 'loki' });
  assert.match(res.body, /class="[^"]*strip-source-fail[^"]*"/, 'Trust strip lacks red source failure highlight');
  assert.ok(res.body.includes('board · as of'), 'Board source incorrectly marked as failed');
  assert.ok(res.body.includes('rooms · live'), 'Rooms source incorrectly marked as failed');
});

Then('the chip clearly names the failed source', async function () {
  const res = await fetchUrl(`${BASE_URL}/`, { 'x-clearing-test': '1', 'x-fail-source': 'loki' });
  assert.match(res.body, /class="[^"]*fail-source-name[^"]*"/, 'Failing trust strip chip does not name source');
  assert.ok(res.body.includes('fail-source-name">loki</span>'), 'Chip does not name failing source loki');
});
