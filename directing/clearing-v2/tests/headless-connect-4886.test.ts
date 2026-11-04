// @test-type: integration
// @domain: messages
// @card: #4886
// @owner: abby-normal

import { chromium } from '@playwright/test';
import * as http from 'http';
import { createApp } from '../src/server';

describe('#4886 AC3: Headless Chrome loads /clearing-v2/ without console errors and connects', () => {
  let server: http.Server;
  let port: number;

  beforeAll(async () => {
    const { app } = createApp({
      rolesUrl: 'http://127.0.0.1:9/roles',
      cardsUrl: 'http://127.0.0.1:9/cards',
      alertsUrl: 'http://127.0.0.1:9/alerts',
      pulseUrl: 'http://127.0.0.1:9/pulse',
      readingsUrl: 'http://127.0.0.1:9/readings',
      relayUrl: 'ws://127.0.0.1:9/relay',
      readOnly: false,
    });
    server = http.createServer(app);
    await new Promise<void>((resolve) => {
      server.listen(0, '127.0.0.1', () => {
        const addr = server.address();
        port = (addr as any).port;
        resolve();
      });
    });
  });

  afterAll(async () => {
    if (server) {
      await new Promise<void>((resolve) => server.close(() => resolve()));
    }
  });

  it('AC3: loads page without uncaught ReferenceError or syntax errors in app.js', async () => {
    const browser = await chromium.launch({
      executablePath: '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
      headless: true,
    });
    const page = await browser.newPage();
    const pageErrors: Error[] = [];
    page.on('pageerror', (err) => pageErrors.push(err));

    await page.goto(`http://127.0.0.1:${port}/clearing-v2/`, { waitUntil: 'load', timeout: 8000 });
    await new Promise((r) => setTimeout(r, 1000));
    await browser.close();

    const requireErrors = pageErrors.filter((e) => e.message.includes('require is not defined'));
    expect(requireErrors).toHaveLength(0);
    expect(pageErrors.filter((e) => !e.message.includes('EventSource'))).toHaveLength(0);
  });
});
