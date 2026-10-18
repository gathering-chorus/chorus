// @test-type: unit
// @domain: cards
// @card: #4598
// @owner: abby-normal
// #4598 — Tap a card in Clearing v2 to see its details and live pipeline state, so Jeff stops asking /cws.

import http from 'http';
import fs from 'fs';
import path from 'path';
import { createApp } from '../src/server';
import { parsePipelineOutput } from '../src/sources/pipeline';
import { toDrawerCard } from '../src/sources/card';
import { renderBoardPage } from '../src/views';

describe('Card #4598: Card Details & Live /cws Pipeline State in Clearing v2', () => {
  const FAILED_CWS_OUTPUT = `#4593 abby-normal failed at commit                     (20:35) · cicd run 1
  lead 2h12m (filed 10-17 19:00) · cycle 48m (WIP 10-17 20:24)

  v2 failed · 1 run · dagu 034hln0tWUmo1jFU0GrQxc
  pipeline  run  start        finish  result      step
  cicd        1  10-17 20:35  20:35   failed      skill-werk-commit  — werk-commit: git rev-parse --abbrev-ref HEAD: fatal: detected dubious ownership in reposit

  tests (cicd run 1): unreadable — Loki did not answer at http://localhost:3102`;

  const RUNNING_CWS_OUTPUT = `#4598 abby-normal running                           (21:11) · cicd run 1 · skill-werk-test
  lead 16m (filed 10-17 20:56) · cycle 1m (WIP 10-17 21:11)

  v2 running · 1 run · dagu abc123runId
  pipeline  run  start        finish  result      step
  cicd        1  10-17 21:11  -       running     skill-werk-test

  tests (cicd run 1): 2 ok (2 of 5) (live)`;

  const WAITING_CWS_OUTPUT = `#4598 abby-normal waiting                           (21:15) · cicd run 1 · waiting for your go
  lead 19m (filed 10-17 20:56) · cycle 4m (WIP 10-17 21:11)

  v2 waiting · 1 run · dagu abc123runId
  pipeline  run  start        finish  result      step
  cicd        1  10-17 21:11  -       waiting     skill-werk-go

  tests (cicd run 1): 5 ok (5 of 5)`;

  test('parsePipelineOutput parses running run as isLive: true with step and motion state', () => {
    const res = parsePipelineOutput(4598, RUNNING_CWS_OUTPUT);
    expect(res.cardId).toBe(4598);
    expect(res.isLive).toBe(true);
    expect(res.status).toBe('running');
    expect(res.step).toBe('skill-werk-test');
    expect(res.output).toBe(RUNNING_CWS_OUTPUT);
  });

  test('parsePipelineOutput parses waiting run as isLive: true', () => {
    const res = parsePipelineOutput(4598, WAITING_CWS_OUTPUT);
    expect(res.cardId).toBe(4598);
    expect(res.isLive).toBe(true);
    expect(res.status).toBe('waiting');
  });

  // MANDATORY AC NEGATIVE PROOF:
  // "Negative proof: a card with a failed run shows the failed step and reason, not 'running'"
  test('NEGATIVE PROOF: a card with a failed run shows failed step and reason, isLive is false, and status is NOT running', () => {
    const res = parsePipelineOutput(4593, FAILED_CWS_OUTPUT);
    expect(res.cardId).toBe(4593);
    expect(res.status).toBe('failed');
    expect(res.status).not.toBe('running');
    expect(res.isLive).toBe(false);
    expect(res.failedStep).toBe('skill-werk-commit');
    expect(res.failedReason).toContain('fatal: detected dubious ownership in reposit');
    expect(res.output).toContain('failed');
  });

  test('toDrawerCard parses title, owner, status, full description, and AC items', () => {
    const rawData = {
      kind: 'Card',
      data: {
        title: 'Tap a card in Clearing v2',
        status: 'WIP',
        owner: 'Abby-normal',
        description: '## Why this matters\nJeff wants to stop asking /cws\n\n## AC\n- [ ] Tapping opens card\n- [x] Tested at phone width',
        ac_items: [
          { text: 'Tapping opens card', checked: false },
          { text: 'Tested at phone width', checked: true },
        ],
      },
    };
    const card = toDrawerCard(4598, rawData);
    expect(card).not.toBeNull();
    expect(card?.title).toBe('Tap a card in Clearing v2');
    expect(card?.owner).toBe('Abby Normal');
    expect(card?.status).toBe('WIP');
    expect(card?.description).toContain('Jeff wants to stop asking /cws');
    expect(card?.acItems).toEqual([
      { text: 'Tapping opens card', checked: false },
      { text: 'Tested at phone width', checked: true },
    ]);
  });

  test('/api/cards/:id returns card description and AC items', async () => {
    const cardsStub = http.createServer((req, res) => {
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({
        kind: 'Card',
        data: {
          title: 'Card 4598 Title',
          status: 'WIP',
          owner: 'Abby-normal',
          description: 'Full description for 4598\n- [ ] AC item 1',
          ac_items: [{ text: 'AC item 1', checked: false }],
        },
      }));
    });
    await new Promise<void>((r) => cardsStub.listen(0, '127.0.0.1', () => r()));
    const cardsPort = (cardsStub.address() as { port: number }).port;

    const { app } = createApp({
      relayUrl: '',
      pulseUrl: 'http://127.0.0.1:9',
      cardsUrl: `http://127.0.0.1:${cardsPort}`,
      relaySigner: null,
    });
    const server = http.createServer(app);
    await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
    const port = (server.address() as { port: number }).port;

    try {
      const res = await fetch(`http://127.0.0.1:${port}/api/cards/4598`, {
        headers: { 'x-clearing-test': '1', 'x-cards-url': `http://127.0.0.1:${cardsPort}` },
      });
      expect(res.status).toBe(200);
      const json = await res.json() as any;
      expect(json.ok).toBe(true);
      expect(json.card.title).toBe('Card 4598 Title');
      expect(json.card.description).toContain('Full description for 4598');
      expect(json.card.acItems).toEqual([{ text: 'AC item 1', checked: false }]);
    } finally {
      await new Promise<void>((r) => server.close(() => r()));
      await new Promise<void>((r) => cardsStub.close(() => r()));
    }
  });

  test('/api/cards/:id/pipeline returns cws output and live state', async () => {
    const { app } = createApp({
      relayUrl: '',
      pulseUrl: 'http://127.0.0.1:9',
      relaySigner: null,
      runWerkStatus: async (cardId: number) => {
        return cardId === 4593 ? FAILED_CWS_OUTPUT : RUNNING_CWS_OUTPUT;
      },
    });
    const server = http.createServer(app);
    await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
    const port = (server.address() as { port: number }).port;

    try {
      // 1. Running card returns isLive: true and running step
      const resRunning = await fetch(`http://127.0.0.1:${port}/api/cards/4598/pipeline`);
      expect(resRunning.status).toBe(200);
      const jsonRunning = await resRunning.json() as any;
      expect(jsonRunning.ok).toBe(true);
      expect(jsonRunning.pipeline.isLive).toBe(true);
      expect(jsonRunning.pipeline.status).toBe('running');
      expect(jsonRunning.pipeline.step).toBe('skill-werk-test');

      // 2. NEGATIVE PROOF: Failed card returns status: 'failed', isLive: false, and failedStep
      const resFailed = await fetch(`http://127.0.0.1:${port}/api/cards/4593/pipeline`);
      expect(resFailed.status).toBe(200);
      const jsonFailed = await resFailed.json() as any;
      expect(jsonFailed.ok).toBe(true);
      expect(jsonFailed.pipeline.isLive).toBe(false);
      expect(jsonFailed.pipeline.status).toBe('failed');
      expect(jsonFailed.pipeline.status).not.toBe('running');
      expect(jsonFailed.pipeline.failedStep).toBe('skill-werk-commit');
      expect(jsonFailed.pipeline.failedReason).toContain('fatal: detected dubious ownership in reposit');
    } finally {
      await new Promise<void>((r) => server.close(() => r()));
    }
  });

  test('renderBoardPage renders card drawer with description, AC container, and pipeline motion container', () => {
    const html = renderBoardPage(
      [{ name: 'WIP', ok: true, cards: [{ id: 4598, title: 'Tap a card in v2', owner: 'Abby Normal' }] }],
      4598,
      []
    );
    expect(html).toContain('id="card-drawer"');
    expect(html).toContain('id="drawer-card-desc"');
    expect(html).toContain('id="drawer-card-ac"');
    expect(html).toContain('id="drawer-card-cws"');
    expect(html).toContain('id="drawer-card-cws-motion"');
    expect(html).toContain('openCardDrawer(\'4598\')');
    expect(html).toContain('data-open-card="4598"');
  });

  test('phone width CSS ensures card drawer fits phone screen without horizontal blowout', () => {
    const html = renderBoardPage([], null, []);
    expect(html).toContain('viewport-fit=cover');
    const cssPath = path.join(__dirname, '..', 'public', 'style.css');
    const css = fs.readFileSync(cssPath, 'utf8');
    const phoneBlock = css.slice(css.indexOf('@media (max-width: 480px)'));
    
    // Checks that card drawer rules exist in phone block
    expect(phoneBlock).toContain('.card-drawer');
    expect(phoneBlock).toMatch(/max-width:\s*100vw|width:\s*100%/);
  });
});
