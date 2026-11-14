// @test-type: integration:ui
// @domain: awareness
// @card: #4895
// @owner: silas
// #4895 AC5 — the room shows a role a nudge cannot reach, before Jeff has to ask. On 11-06
// Abby read as present while Wren's 09:42 reach check went unanswered. The roles page is
// loaded headless from the served app (the #4885 lesson: prove the page Jeff loads), with
// pulse's actor-state stubbed to say Abby is unreachable and Kade reachable.
import { chromium } from '@playwright/test';
import * as http from 'http';
import { createApp } from '../src/server';
import { toActorRow } from '../src/sources/pulse';

const ACTORS = {
  derivedAt: '2026-11-06T15:00:00Z',
  unmeasured: [],
  actors: [
    { aboutPrincipal: 'principal-abby-normal', name: 'abby-normal', stateValue: 'available', activity: 'idle', cards: [], sessions: [], reachability: 'unreachable' },
    { aboutPrincipal: 'principal-kade', name: 'kade', stateValue: 'building', activity: 'acting', cards: [], sessions: [], reachability: 'reachable' },
  ],
};
const listen = (s: http.Server) => new Promise<number>((r) => s.listen(0, '127.0.0.1', () => r((s.address() as { port: number }).port)));

describe('#4895 AC5: the served roles page tags a role nudges cannot reach', () => {
  it('pulse reachability reaches the roster row verbatim', () => {
    expect(toActorRow(ACTORS.actors[0]).reachability).toBe('unreachable');
    expect(toActorRow({ aboutPrincipal: 'principal-wren', stateValue: 'off' }).reachability).toBeUndefined();
  });

  it('Abby (unreachable) is tagged; Kade (reachable) is not — NEGATIVE PROOF in the same page', async () => {
    const pulse = http.createServer((req, res) => {
      res.setHeader('content-type', 'application/json');
      res.end(req.url?.startsWith('/api/actor-state') ? JSON.stringify(ACTORS) : '{}');
    });
    const pulsePort = await listen(pulse);
    const { app } = createApp({
      rolesUrl: 'http://127.0.0.1:9/roles', cardsUrl: 'http://127.0.0.1:9/cards', alertsUrl: 'http://127.0.0.1:9/alerts',
      pulseUrl: `http://127.0.0.1:${pulsePort}`, readingsUrl: 'http://127.0.0.1:9/readings', relayUrl: 'ws://127.0.0.1:9/relay', readOnly: true,
    });
    const server = http.createServer(app);
    const port = await listen(server);
    const browser = await chromium.launch({
      executablePath: '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
      headless: true,
      args: ['--no-sandbox', '--use-mock-keychain', '--password-store=basic'],
    });
    try {
      const page = await browser.newPage();
      // inside jest's 5 s test budget (#4416: no raised timeouts), so a page that never
      // loads fails here by name; measured 0.9-2.6 s whole test, idle (2026-11-13)
      await page.goto(`http://127.0.0.1:${port}/clearing-v2/roles`, { waitUntil: 'load', timeout: 3000 });
      const tags = await page.locator('.actor-unreachable').allInnerTexts();
      expect(tags).toEqual(['unreachable — nudges to abby-normal will not land']);
      expect(await page.content()).not.toContain('nudges to kade will not land');
    } finally {
      await browser.close();
      await new Promise<void>((r) => server.close(() => r()));
      await new Promise<void>((r) => pulse.close(() => r()));
    }
  // slow-by-design: Google Chrome cold launch + page load measured 0.9-2.6 s idle, over 5 s under the cicd test run (#4965 run 034xBbcP, 2026-11-13); the page itself waits 3 s
  }, 15000);
});
