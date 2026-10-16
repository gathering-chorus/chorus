// @test-type: unit — verifies Today page alerts with their card and decisions owed red by name (#3734); who is working and what changed overnight are pulse's (tests/pulse-presents.test.ts, #4563)
// @card: #4530
// @owner: abby-normal
// @domain: messages

import http from 'http';
import { createApp } from '../src/server';

describe('Today AC: Alerts with card, Decisions owed red by name (#3734)', () => {
  let stubServer: http.Server;
  let stubPort: number;

  beforeAll(async () => {
    await new Promise<void>((resolve) => {
      stubServer = http.createServer((req, res) => {
        const url = req.url || '';

        // Pulse: no actors, no changes, no readings (these panels are covered in pulse-presents.test.ts)
        if (url.startsWith('/pulse/api/')) {
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ actors: [], cards: [], readings: [], unmeasured: [] }));
          return;
        }

        // 6. Stub Alerts Door
        if (url.startsWith('/fact-alerts-empty')) {
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ data: [] }));
          return;
        }
        if (url.startsWith('/fact-alerts')) {
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({
            data: [
              {
                id: 'alert-swap-1',
                label: 'Library swap pressure > 20GB',
                status: 'firing',
                resolvingCard: 9955,
                meta: 'new · #9955',
              },
              {
                id: 'alert-backup-2',
                label: 'Fuseki backup stale > 24h',
                status: 'acked',
                resolvingCard: 9966,
                meta: 'changed: Silas acked 09:15 · #9966',
              },
              {
                id: 'alert-resolved-3',
                label: 'Disk space recovered',
                status: 'resolved',
                resolvingCard: 9977,
                meta: 'resolved · #9977',
              },
            ],
          }));
          return;
        }

        res.writeHead(404).end();
      });

      stubServer.listen(0, '127.0.0.1', () => {
        stubPort = (stubServer.address() as any).port;
        resolve();
      });
    });
  });

  afterAll(async () => {
    await new Promise<void>((r) => stubServer.close(() => r()));
  });

  test('1. NEGATIVE PROOF: Alerts render dynamically with resolving card links (#3734)', async () => {
    // 2A: Alerts door returns dynamic unseen alert instances
    const appInstance = createApp({
      relayUrl: 'ws://127.0.0.1:3499',
      pulseUrl: `http://127.0.0.1:${stubPort}/pulse`,
      alertsUrl: `http://127.0.0.1:${stubPort}/fact-alerts`,
    });
    const s = http.createServer(appInstance.app);
    await new Promise<void>((r) => s.listen(0, '127.0.0.1', () => r()));
    const p = (s.address() as any).port;

    try {
      const res = await fetch(`http://127.0.0.1:${p}/today`, {
        headers: {
          'x-alerts-url': `http://127.0.0.1:${stubPort}/fact-alerts`,
        },
      });
      expect(res.status).toBe(200);
      const html = await res.text();

      // Alerts section exists and reflects active count
      expect(html).toContain('Alerts · new or changed only');

      // Dynamic firing alert rendered with resolving card link
      expect(html).toContain('Library swap pressure > 20GB');
      expect(html).toContain('firing');
      expect(html).toContain('href="/board?card=9955"');
      expect(html).toContain('#9955');

      // Dynamic acked alert rendered with resolving card link
      expect(html).toContain('Fuseki backup stale > 24h');
      expect(html).toContain('acked');
      expect(html).toContain('href="/board?card=9966"');
      expect(html).toContain('#9966');

      // NEGATIVE PROOF: Resolved alert is excluded
      expect(html).not.toContain('Disk space recovered');
      expect(html).not.toContain('#9977');

      // NEGATIVE PROOF: Zero hardcoded mock literals
      expect(html).not.toContain('Relay write-backs lost · 14 since 03:10');
      expect(html).not.toContain('#4366');
      expect(html).not.toContain('Fuseki backup older than 26h');
      expect(html).not.toContain('#4171');
    } finally {
      await new Promise<void>((r) => s.close(() => r()));
    }

    // 2B: Negative proof when alerts source is empty / unmodeled (Part 1 state)
    const appEmpty = createApp({
      relayUrl: 'ws://127.0.0.1:3499',
      pulseUrl: `http://127.0.0.1:${stubPort}/pulse`,
      alertsUrl: `http://127.0.0.1:${stubPort}/fact-alerts-empty`,
    });
    const sEmpty = http.createServer(appEmpty.app);
    await new Promise<void>((r) => sEmpty.listen(0, '127.0.0.1', () => r()));
    const pEmpty = (sEmpty.address() as any).port;

    try {
      const res = await fetch(`http://127.0.0.1:${pEmpty}/today`, {
        headers: {
          'x-alerts-url': `http://127.0.0.1:${stubPort}/fact-alerts-empty`,
        },
      });
      expect(res.status).toBe(200);
      const html = await res.text();

      // Must show honest no source (part 2), NEVER fake mock alerts
      expect(html).toContain('no source (part 2)');
      expect(html).not.toContain('0 active alerts');
      expect(html).toContain('Alerts · no source (part 2)');
      expect(html).not.toContain('alert-resolving-card');
      expect(html).not.toContain('Relay write-backs lost');
      expect(html).not.toContain('#4366');
      expect(html).not.toContain('#4171');
    } finally {
      await new Promise<void>((r) => sEmpty.close(() => r()));
    }
  });

  test('2. NEGATIVE PROOF: Decisions owed stays red by name (must read no source (part 2))', async () => {
    const appInstance = createApp({
      relayUrl: 'ws://127.0.0.1:3499',
      pulseUrl: `http://127.0.0.1:${stubPort}/pulse`,
    });
    const s = http.createServer(appInstance.app);
    await new Promise<void>((r) => s.listen(0, '127.0.0.1', () => r()));
    const p = (s.address() as any).port;

    try {
      const res = await fetch(`http://127.0.0.1:${p}/today`);
      expect(res.status).toBe(200);
      const html = await res.text();

      // Decisions owed stays red by name: must read "no source (part 2)", NOT "0 owed"
      expect(html).toContain('Decisions owed · no source (part 2)');
      expect(html).not.toContain('Decisions owed · 0');
      expect(html).not.toContain('0 decisions owed');
      expect(html).toContain('no source (part 2)');
      // Must NOT contain fake approved decisions
      expect(html).not.toContain('Approve all');
    } finally {
      await new Promise<void>((r) => s.close(() => r()));
    }
  });
});
