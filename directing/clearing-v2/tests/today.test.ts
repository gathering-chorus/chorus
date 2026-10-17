// @test-type: unit — verifies Today page alerts (trigger + data, empty = 0 firing, unreadable = unmeasured with reason) and decisions owed red by name (#3734, #4574); no live services.
// @card: #4574
// @owner: abby-normal
// @domain: messages

import http from "http";
import { createApp } from "../src/server";

describe("Today AC: Alerts from API (trigger + data, 0 firing, unmeasured) & Decisions owed (#4574)", () => {
  let stubServer: http.Server;
  let stubPort: number;

  beforeAll(async () => {
    await new Promise<void>((resolve) => {
      stubServer = http.createServer((req, res) => {
        const url = req.url || "";

        if (url.startsWith("/pulse/api/")) {
          res.writeHead(200, { "Content-Type": "application/json" });
          res.end(JSON.stringify({ actors: [], cards: [], readings: [], unmeasured: [] }));
          return;
        }

        if (url.startsWith("/fact-alerts-empty")) {
          res.writeHead(200, { "Content-Type": "application/json" });
          res.end(JSON.stringify({ data: { total: 0, alerts: [] } }));
          return;
        }

        if (url.startsWith("/fact-alerts-503")) {
          res.writeHead(503, { "Content-Type": "application/json" });
          res.end(JSON.stringify({ error: "No pulse snapshot available; alert state unknown." }));
          return;
        }

        if (url.startsWith("/fact-alerts-context")) {
          res.writeHead(200, { "Content-Type": "application/json" });
          res.end(JSON.stringify({
            domain: "chorus",
            source: "/api/chorus/context/alerts",
            data: {
              total: 2,
              alerts: [
                {
                  name: "chorus-mcp-down",
                  severity: "critical",
                  description: "tool runner (:3342) /health not answering",
                },
                {
                  name: "host-pressure",
                  severity: "warning",
                  trigger: "host-pressure: swap_used_gb > 16",
                  data: "swap_used_gb=18.4, load1=9.2",
                },
              ],
            },
          }));
          return;
        }

        if (url.startsWith("/fact-alerts")) {
          res.writeHead(200, { "Content-Type": "application/json" });
          res.end(JSON.stringify({
            data: [
              {
                id: "alert-swap-1",
                label: "Library swap pressure > 20GB",
                status: "firing",
                resolvingCard: 9955,
                meta: "new · #9955",
              },
              {
                id: "alert-backup-2",
                label: "Fuseki backup stale > 24h",
                status: "acked",
                resolvingCard: 9966,
                meta: "changed: Silas acked 09:15 · #9966",
              },
              {
                id: "alert-resolved-3",
                label: "Disk space recovered",
                status: "resolved",
                resolvingCard: 9977,
                meta: "resolved · #9977",
              },
            ],
          }));
          return;
        }

        res.writeHead(404).end();
      });

      stubServer.listen(0, "127.0.0.1", () => {
        stubPort = (stubServer.address() as any).port;
        resolve();
      });
    });
  });

  afterAll(async () => {
    await new Promise<void>((r) => stubServer.close(() => r()));
  });

  test("1. Alerts from /api/chorus/context/alerts keep rows without status field and render trigger + data (#4574)", async () => {
    const appInstance = createApp({
      relayUrl: "ws://127.0.0.1:3499",
      pulseUrl: `http://127.0.0.1:${stubPort}/pulse`,
      alertsUrl: `http://127.0.0.1:${stubPort}/fact-alerts-context`,
      relaySigner: null,
    });
    const s = http.createServer(appInstance.app);
    await new Promise<void>((r) => s.listen(0, "127.0.0.1", () => r()));
    const p = (s.address() as any).port;

    try {
      const res = await fetch(`http://127.0.0.1:${p}/today`);
      expect(res.status).toBe(200);
      const html = await res.text();

      expect(html).toContain("chorus-mcp-down");
      expect(html).toContain("no trigger data yet"); // #4574: the description is the why; never shown
      expect(html).not.toContain("tool runner (:3342) /health not answering");
      expect(html).toContain("host-pressure: swap_used_gb > 16");
      expect(html).toContain("swap_used_gb=18.4, load1=9.2");
      expect(html).not.toContain("Alerts · no source (part 2)");
    } finally {
      await new Promise<void>((r) => s.close(() => r()));
    }
  });

  test("2. Alerts with status firing/acked render resolving card links; resolved alerts are excluded (#3734)", async () => {
    const appInstance = createApp({
      relayUrl: "ws://127.0.0.1:3499",
      pulseUrl: `http://127.0.0.1:${stubPort}/pulse`,
      alertsUrl: `http://127.0.0.1:${stubPort}/fact-alerts`,
      relaySigner: null,
    });
    const s = http.createServer(appInstance.app);
    await new Promise<void>((r) => s.listen(0, "127.0.0.1", () => r()));
    const p = (s.address() as any).port;

    try {
      const res = await fetch(`http://127.0.0.1:${p}/today`);
      expect(res.status).toBe(200);
      const html = await res.text();

      expect(html).toContain("Library swap pressure > 20GB");
      expect(html).toContain("firing");
      expect(html).toContain('href="/board?card=9955"');
      expect(html).toContain("#9955");

      expect(html).toContain("Fuseki backup stale > 24h");
      expect(html).toContain("acked");
      expect(html).toContain('href="/board?card=9966"');
      expect(html).toContain("#9966");

      // NEGATIVE PROOF: Resolved alert is excluded
      expect(html).not.toContain("Disk space recovered");
      expect(html).not.toContain("#9977");
    } finally {
      await new Promise<void>((r) => s.close(() => r()));
    }
  });

  test("3. Empty alerts response shows 0 firing, not unmeasured or no source (#4574)", async () => {
    const appEmpty = createApp({
      relayUrl: "ws://127.0.0.1:3499",
      pulseUrl: `http://127.0.0.1:${stubPort}/pulse`,
      alertsUrl: `http://127.0.0.1:${stubPort}/fact-alerts-empty`,
      relaySigner: null,
    });
    const sEmpty = http.createServer(appEmpty.app);
    await new Promise<void>((r) => sEmpty.listen(0, "127.0.0.1", () => r()));
    const pEmpty = (sEmpty.address() as any).port;

    try {
      const res = await fetch(`http://127.0.0.1:${pEmpty}/today`);
      expect(res.status).toBe(200);
      const html = await res.text();

      expect(html).toContain("0 firing");
      expect(html).not.toContain("Alerts · no source (part 2)");
      expect(html).not.toContain("alerts-unmeasured");
    } finally {
      await new Promise<void>((r) => sEmpty.close(() => r()));
    }
  });

  test("4. NEGATIVE PROOF: Unreadable alerts source reads unmeasured with reason, never 0 firing or empty (#4574)", async () => {
    const appDown = createApp({
      relayUrl: "ws://127.0.0.1:3499",
      pulseUrl: `http://127.0.0.1:${stubPort}/pulse`,
      alertsUrl: `http://127.0.0.1:${stubPort}/fact-alerts-503`,
      relaySigner: null,
    });
    const sDown = http.createServer(appDown.app);
    await new Promise<void>((r) => sDown.listen(0, "127.0.0.1", () => r()));
    const pDown = (sDown.address() as any).port;

    try {
      const res = await fetch(`http://127.0.0.1:${pDown}/today`);
      expect(res.status).toBe(200);
      const html = await res.text();

      expect(html).toContain("alerts-unmeasured");
      expect(html).toContain("unmeasured:");
      expect(html).toContain("status 503");
      expect(html).not.toContain("0 firing");
      expect(html).not.toContain("Alerts · no source (part 2)");
    } finally {
      await new Promise<void>((r) => sDown.close(() => r()));
    }
  });

  test("5. NEGATIVE PROOF: Decisions owed stays red by name (must read no source (part 2))", async () => {
    const appInstance = createApp({
      relayUrl: "ws://127.0.0.1:3499",
      pulseUrl: `http://127.0.0.1:${stubPort}/pulse`,
      alertsUrl: `http://127.0.0.1:${stubPort}/fact-alerts-empty`,
      relaySigner: null,
    });
    const s = http.createServer(appInstance.app);
    await new Promise<void>((r) => s.listen(0, "127.0.0.1", () => r()));
    const p = (s.address() as any).port;

    try {
      const res = await fetch(`http://127.0.0.1:${p}/today`);
      expect(res.status).toBe(200);
      const html = await res.text();

      expect(html).toContain("Decisions owed · no source (part 2)");
      expect(html).not.toContain("Decisions owed · 0");
      expect(html).not.toContain("0 decisions owed");
      expect(html).not.toContain("Approve all");
    } finally {
      await new Promise<void>((r) => s.close(() => r()));
    }
  });
});

// #4574 — Jeff 10-16: an alert is the trigger then the data, never the why. Silas is adding
// trigger + data to the alerts API; until a row carries data, the page says so.
describe('alert rows never show the description', () => {
  test('negative proof: a row with only name + description shows the name and "no trigger data yet"', async () => {
    const http = await import('http');
    const { fetchAlerts } = await import('../src/sources/today');
    const body = { data: { total: 1, alerts: [{ name: 'chorus-api-eventloop', severity: 'warning', description: 'event loop blocked because of X' }] } };
    const s = http.createServer((_q, r) => { r.writeHead(200, { 'Content-Type': 'application/json' }); r.end(JSON.stringify(body)); });
    await new Promise<void>((r) => s.listen(0, '127.0.0.1', () => r()));
    try {
      const read = await fetchAlerts(`http://127.0.0.1:${(s.address() as { port: number }).port}/alerts`);
      const text = JSON.stringify(read);
      expect(text).toContain('chorus-api-eventloop');
      expect(text).toContain('no trigger data yet');
      expect(text).not.toContain('because of X');
    } finally {
      await new Promise<void>((r) => s.close(() => r()));
    }
  });
});
