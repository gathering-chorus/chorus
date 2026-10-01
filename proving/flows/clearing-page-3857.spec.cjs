// @test-type: e2e:ui — playwright drives the Clearing page in a browser against its own Clearing (lib/own-clearing.cjs)
// @card: #4417
// @owner: wren
/**
 * #4417 — the page behaviours that directing/clearing/tests used to check by
 * matching regexes over public/index.html (#3857 batch, #4234). Each case here
 * loads the real page, phone-sized where the behaviour is a phone behaviour.
 */
const { test, expect, devices } = require('@playwright/test');
const { ownClearing } = require('./lib/own-clearing.cjs');

const CLEARING_TARGET = ownClearing(test);
const CLEARING = CLEARING_TARGET.url;
const PHONE = { viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true, userAgent: devices['iPhone 13'].userAgent };

async function open(page) {
  await page.goto(CLEARING, { waitUntil: 'domcontentloaded' });
  await page.waitForFunction(() => typeof socket !== 'undefined' && socket.connected, null, { timeout: 20000 });
}

async function postAs(request, from, text) {
  return request.post(`${CLEARING}/api/message`, { headers: CLEARING_TARGET.auth, data: { from, text } });
}

test.describe('#4234 a readout in a fence stays readable on a phone', () => {
  test.use(PHONE);
  test('the block keeps rows on one line and scrolls sideways; the message row does not clip it', async ({ page, request }) => {
    await open(page);
    const longLine = `row-missing-required-field 16161 v1-row 15862 owner-not-principal 6783 ${Date.now()}`;
    await postAs(request, 'wren', '```\n' + longLine + '\n```');
    const pre = page.locator('#messages pre', { hasText: longLine }).last();
    await expect(pre).toBeVisible({ timeout: 20000 });
    const s = await pre.evaluate((el) => {
      const cs = getComputedStyle(el);
      const msg = el.closest('.msg');
      return { ws: cs.whiteSpace, ox: cs.overflowX, scrolls: el.scrollWidth > el.clientWidth, msgOx: msg ? getComputedStyle(msg).overflowX : 'none' };
    });
    expect(s.ws).toBe('pre');
    expect(s.ox).toBe('auto');
    expect(s.scrolls).toBe(true);
    expect(s.msgOx).not.toBe('hidden');
    // and the PAGE does not get wider than the phone: the block scrolls, the room does not
    const page_ = await page.evaluate(() => ({ sw: document.documentElement.scrollWidth, vw: visualViewport ? visualViewport.width : innerWidth, ih: innerHeight, vh: visualViewport ? visualViewport.height : innerHeight }));
    expect(page_.sw).toBeLessThanOrEqual(Math.ceil(page_.vw));
  });
});

test.describe('#3857 typing on a phone', () => {
  test.use(PHONE);
  test('the box is a textarea, not focused on load, Enter adds a line instead of sending, and it grows', async ({ page }) => {
    await open(page);
    const input = page.locator('#input');
    expect(await input.evaluate((el) => el.tagName)).toBe('TEXTAREA');
    expect(await page.evaluate(() => document.activeElement && document.activeElement.id)).not.toBe('input');
    await expect(page.locator('.mobile-hint')).toBeVisible();
    await expect(page.locator('.desktop-hint')).toBeHidden();
    const h0 = await input.evaluate((el) => el.offsetHeight);
    await input.click();
    await input.pressSequentially('line one');
    for (const l of ['two', 'three', 'four']) { await input.press('Enter'); await input.pressSequentially(l); }
    await expect(input).toHaveValue('line one\ntwo\nthree\nfour');
    await expect(page.locator('#send-btn')).toHaveText('Send');
    expect(await input.evaluate((el) => el.offsetHeight)).toBeGreaterThan(h0);
  });
});

test.describe('#3857 typing at a desk', () => {
  test('NEGATIVE PROOF: on a desktop, Enter sends and Shift+Enter does not', async ({ page }) => {
    await open(page);
    await expect(page.locator('.desktop-hint')).toBeVisible();
    const input = page.locator('#input');
    await input.click();
    await input.pressSequentially('first');
    await input.press('Shift+Enter');
    await expect(input).toHaveValue('first\n');
    await input.pressSequentially(`desk-${Date.now()}`);
    await input.press('Enter');
    await expect(input).toHaveValue('');
    await expect(page.locator('#send-btn')).toHaveText('Sent', { timeout: 10000 });
  });
});

test.describe('#3857 the connection light tells the truth', () => {
  test('connected on load; a dropped connection shows reconnecting; it comes back connected', async ({ page, context }) => {
    await open(page);
    const status = page.locator('#connection-status');
    await expect(status).toContainText('connected');
    await expect(status.locator('.dot-connected')).toHaveCount(1);
    await page.evaluate(() => socket.io.engine.close());
    await expect(status).toContainText(/reconnecting|connection lost/, { timeout: 10000 });
    await expect(status.locator('.dot-disconnected')).toHaveCount(1);
    await expect(status).toContainText(/^\s*connected/, { timeout: 20000 });
  });
});

test.describe('#4417 the room is never wider than the phone', () => {
  test.use(PHONE);
  // Found by the streams-tab case below: once the tiles filled in, the kade tile
  // was 418px on a 390px screen and the page went sideways.
  test('after the tiles fill in, nothing is wider than the screen', async ({ page }) => {
    await open(page);
    await page.waitForTimeout(6000); // tiles refresh every 5s
    const r = await page.evaluate(() => ({
      sw: document.documentElement.scrollWidth,
      w: window.visualViewport ? window.visualViewport.width : innerWidth,
      tiles: [...document.querySelectorAll('#tiles .tile')].map((e) => Math.round(e.getBoundingClientRect().right)),
    }));
    for (const right of r.tiles) expect(right).toBeLessThanOrEqual(Math.ceil(r.w));
    expect(r.sw).toBeLessThanOrEqual(Math.ceil(r.w));
  });
});

test.describe('#3857 the streams tab on a phone', () => {
  test.use(PHONE);
  test('opening Streams covers the room above the tabs and input, on an opaque background', async ({ page }) => {
    await open(page);
    await page.waitForLoadState('load');
    await page.click('.mobile-tab[data-panel="streams"]');
    const box = await page.locator('#mobile-stream').evaluate((el) => {
      const cs = getComputedStyle(el);
      const r = el.getBoundingClientRect();
      const tabs = document.getElementById('mobile-tabs').getBoundingClientRect();
      return { display: cs.display, position: cs.position, top: r.top, bottom: r.bottom, tabsTop: tabs.top, bg: cs.backgroundColor };
    });
    expect(box.display).toBe('block');
    expect(box.position).toBe('fixed');
    expect(box.top).toBe(0);
    expect(box.bottom).toBeLessThanOrEqual(box.tabsTop + 1);
    expect(box.bg).not.toBe('rgba(0, 0, 0, 0)');
  });
});

// #4417 — the domain panel, from role-filter-badge-3857 and domain-subtotals-3857
// (regexes over index.html). The board answer is stubbed at /api/flow (the
// panel's one source) so the counts are known: 5 chorus cards, 3 kade and 2
// wren; one kade card has no sequence.
const FLOW = {
  domains: {
    chorus: {
      counts: { activeCards: 5, activeTotal: 5, wip: 1 },
      cards: [
        { id: '9001', owner: 'kade', status: 'WIP', priority: 1, title: 'kade clearing one', sequences: ['clearing'] },
        { id: '9002', owner: 'kade', status: 'Next', priority: 2, title: 'kade clearing two', sequences: ['clearing'] },
        { id: '9003', owner: 'kade', status: 'Next', priority: 2, title: 'kade with no sequence' },
        { id: '9004', owner: 'wren', status: 'Next', priority: 2, title: 'wren cards one', sequences: ['cards'] },
        { id: '9005', owner: 'wren', status: 'Next', priority: 3, title: 'wren cards two', sequences: ['cards'] },
      ],
    },
  },
};

test.describe('#3857 the domain panel counts what it shows', () => {
  test.beforeEach(async ({ page }) => {
    await page.route('**/api/flow', (route) => route.fulfill({ contentType: 'application/json', body: JSON.stringify(FLOW) }));
    await open(page);
    await page.locator('.flow-section-title', { hasText: 'chorus' }).first().waitFor({ timeout: 20000 });
  });

  const header = (page) => page.locator('#flow-content > .flow-section > .flow-section-title', { hasText: 'chorus' });
  const subCounts = (page) => page.locator('#flow-content .flow-section .flow-section-cards > div > .flow-section-title > span:first-of-type')
    .evaluateAll((els) => els.map((e) => Number(e.textContent.trim())));

  test('unfiltered: 5 cards, sub-groups by sequence with "unsequenced" last, and they add up', async ({ page }) => {
    await expect(header(page)).toContainText('5 cards');
    await expect(header(page)).toContainText('5 total');
    const titles = await page.locator('#flow-content .flow-section .flow-section-cards > div > .flow-section-title').allTextContents();
    expect(titles.map((t) => t.replace(/[▸▾]/g, '').trim().split(/\s+/)[0])).toEqual(['cards', 'clearing', 'unsequenced']);
    expect((await subCounts(page)).reduce((a, b) => a + b, 0)).toBe(5);
  });

  test('NEGATIVE PROOF: the Kade filter shows 3 cards in the badges and the sub-groups', async ({ page }) => {
    await page.click('.role-filter-btn[data-filter="kade"]');
    await expect(header(page)).toContainText('3 cards');
    await expect(header(page)).toContainText('3 total');
    expect((await subCounts(page)).reduce((a, b) => a + b, 0)).toBe(3);
  });
});

// #4417 — voice capture, page half (from voice-capture-3857's index.html regexes).
// The browser has no microphone here, so getUserMedia and MediaRecorder are
// stand-ins that hand over one chunk of bytes; /api/voice is answered with a
// transcript (the server half is driven in directing/clearing voice-capture-3857).
test.describe('#3857 talking to the room', () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(() => {
      navigator.mediaDevices.getUserMedia = async () => ({ getAudioTracks: () => [{}], getTracks: () => [{ stop() {} }] });
      window.MediaRecorder = class {
        static isTypeSupported() { return true; }
        constructor() { this.state = 'inactive'; }
        start() { this.state = 'recording'; }
        stop() {
          this.state = 'inactive';
          this.ondataavailable && this.ondataavailable({ data: new Blob(['fake-audio'], { type: 'audio/webm' }) });
          this.onstop && this.onstop();
        }
      };
    });
  });

  test('tap to record, tap to stop: the audio is uploaded and the words arrive as his message, no Send tap', async ({ page }) => {
    const said = `voice note ${Date.now()}`;
    let uploaded = 0;
    await page.route('**/api/voice', async (route) => {
      uploaded = (route.request().postDataBuffer() || Buffer.alloc(0)).length;
      await route.fulfill({ contentType: 'application/json', body: JSON.stringify({ transcript: said, audioFile: '/audio-uploads/x.webm' }) });
    });
    await open(page);
    const mic = page.locator('#mic-btn');
    await mic.click();
    await expect(mic).toHaveClass(/recording/);
    await mic.click();
    await expect(mic).not.toHaveClass(/recording/);
    await expect(page.locator('#messages')).toContainText(said, { timeout: 10000 });
    expect(uploaded).toBeGreaterThan(0);
  });

  test('NEGATIVE PROOF: a failed transcription sends nothing and says so in the box', async ({ page }) => {
    await page.route('**/api/voice', (route) => route.fulfill({ contentType: 'application/json', body: JSON.stringify({ error: 'Transcription failed: stub' }) }));
    await open(page);
    const before = await page.locator('#messages .msg').count();
    await page.locator('#mic-btn').click();
    await page.locator('#mic-btn').click();
    await expect(page.locator('#input')).toHaveAttribute('placeholder', /Transcription failed/, { timeout: 10000 });
    await page.waitForTimeout(1000);
    expect(await page.locator('#messages .msg').count()).toBe(before);
  });
});
