// @test-type: integration:ui
// @domain: messages
// @card: #4886
// @owner: abby-normal

import { chromium } from '@playwright/test';
import * as http from 'http';
import express from 'express';
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

  it('AC3: loads page without uncaught ReferenceError and leaves Connecting...', async () => {
    const browser = await chromium.launch({
      executablePath: '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
      headless: true,
      args: ['--no-sandbox', '--disable-setuid-sandbox'],
    });
    try {
      const page = await browser.newPage();
      const pageErrors: Error[] = [];
      const consoleErrors: string[] = [];
      page.on('pageerror', (err) => pageErrors.push(err));
      page.on('console', (msg) => {
        if (msg.type() === 'error') consoleErrors.push(msg.text());
      });

      await page.goto(`http://127.0.0.1:${port}/clearing-v2/`, { waitUntil: 'load', timeout: 15000 });

      // Wait for SSE connect event to update status from "Connecting..."
      await page.waitForFunction('document.getElementById("relay-indicator") && document.getElementById("relay-indicator").textContent !== "Connecting..."', { timeout: 10000 });

      const indicatorText = await page.locator('#relay-indicator').innerText();
      expect(indicatorText).toMatch(/Connected/);

      const requireErrors = pageErrors.filter((e) => e.message.includes('require is not defined'));
      expect(requireErrors).toHaveLength(0);
      expect(pageErrors.filter((e) => !e.message.includes('EventSource'))).toHaveLength(0);
      expect(consoleErrors.filter((c) => c.includes('chorusBoston not loaded'))).toHaveLength(0);
    } finally {
      await browser.close();
    }
  });

  it('AC3 negative proof: without Boston script tag, console error is logged and fallback engages', async () => {
    // Serve HTML that omits the /vendor/chorus-time/boston.js script tag
    const app = express();
    app.get('/clearing-v2/', (_req, res) => {
      res.send(`<!DOCTYPE html><html><head><meta charset="utf-8"></head><body>
        <span class="relay-status" id="relay-indicator">Connecting...</span>
        <div id="messages-stream"></div>
        <script src="/clearing-v2/app.js"></script>
      </body></html>`);
    });
    app.use('/clearing-v2/app.js', express.static(`${__dirname}/../public/app.js`));

    const mockServer = http.createServer(app);
    await new Promise<void>((resolve) => mockServer.listen(0, '127.0.0.1', () => resolve()));
    const mockPort = (mockServer.address() as any).port;

    const browser = await chromium.launch({
      executablePath: '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
      headless: true,
      args: ['--no-sandbox', '--disable-setuid-sandbox'],
    });
    try {
      const page = await browser.newPage();
      const consoleErrors: string[] = [];
      const pageErrors: Error[] = [];
      page.on('console', (msg) => {
        if (msg.type() === 'error') consoleErrors.push(msg.text());
      });
      page.on('pageerror', (err) => pageErrors.push(err));

      await page.goto(`http://127.0.0.1:${mockPort}/clearing-v2/`, { waitUntil: 'load', timeout: 15000 });
      await new Promise((r) => setTimeout(r, 500));

      // 1. Never throws uncaught require is not defined
      expect(pageErrors.some((e) => e.message.includes('require is not defined'))).toBe(false);

      // 2. Surfaced console.error informing about missing chorusBoston
      expect(consoleErrors.some((c) => c.includes('chorusBoston not loaded'))).toBe(true);
    } finally {
      await browser.close();
      await new Promise<void>((resolve) => mockServer.close(() => resolve()));
    }
  });
});
