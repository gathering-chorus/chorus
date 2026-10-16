// @test-type: unit — verifies Today page: who is working (reuse Roles), alerts with their card, changes since Jeff's last session, decisions owed red by name (#3734)
// @card: #4530
// @owner: abby-normal
// @domain: messages

import http from 'http';
import { createApp } from '../src/server';

describe('Today AC: Who is working, Alerts with card, Changes since Jeff session, Decisions owed red by name (#3734)', () => {
  let stubServer: http.Server;
  let stubPort: number;
  let cardsQueryUrl: string | null = null;
  let boardWipCalls = 0;
  let boardNextCalls = 0;

  beforeAll(async () => {
    await new Promise<void>((resolve) => {
      stubServer = http.createServer((req, res) => {
        const url = req.url || '';

        // 1. Stub Roles Door
        if (url.startsWith('/fact-roles')) {
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({
            data: [
              { name: 'kade', label: 'role-kade', roleKind: 'agent' },
              { name: 'wren', label: 'role-wren', roleKind: 'agent' },
              { name: 'silas', label: 'role-silas', roleKind: 'agent' },
              { name: 'abby-normal', label: 'role-abby-normal', roleKind: 'agent' },
            ],
          }));
          return;
        }

        // 2. Stub Sessions Door:
        // - Jeff's last closed session ended at 2026-10-14T20:00:00Z
        // - Kade and Wren have open sessions
        if (url.startsWith('/fact-sessions')) {
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({
            data: [
              {
                name: 'jeff-browser-1',
                channel: 'browser',
                ownedBy: 'principal-jeff',
                actsAs: 'role-jeff',
                sessionState: 'closed',
                endedAt: '2026-10-14T20:00:00Z',
              },
              {
                name: 'kade-session-1',
                actsAs: 'role-kade',
                sessionState: 'open',
              },
              {
                name: 'wren-session-1',
                actsAs: 'role-wren',
                sessionState: 'open',
              },
            ],
          }));
          return;
        }

        // 3. Stub Loki:
        // - Kade has action in last 5m (count5m = 5) -> state = 'working'
        // - Wren has action in 30m, but 0 in 5m -> state = 'idle'
        // - Silas has closed session -> state = 'off'
        if (url.startsWith('/fact-loki/ready')) {
          res.writeHead(200, { 'Content-Type': 'text/plain' });
          res.end('ready');
          return;
        }
        if (url.startsWith('/fact-loki/loki/api/v1/query')) {
          const is5m = url.includes('%5B5m%5D') || url.includes('[5m]');
          if (is5m) {
            res.writeHead(200, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({
              status: 'success',
              data: {
                resultType: 'vector',
                result: [{ metric: { role: 'kade' }, value: [Math.floor(Date.now() / 1000), '5'] }],
              },
            }));
            return;
          } else {
            res.writeHead(200, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({
              status: 'success',
              data: {
                resultType: 'vector',
                result: [
                  { metric: { role: 'kade' }, value: [Math.floor(Date.now() / 1000), '15'] },
                  { metric: { role: 'wren' }, value: [Math.floor(Date.now() / 1000), '8'] },
                ],
              },
            }));
            return;
          }
        }

        // 4. Stub Cards Door:
        // Card #4536: modified 2026-10-15T02:00:00Z (after Jeff's session ended at 2026-10-14T20:00:00Z)
        // Card #4530: modified 2026-10-15T01:00:00Z (after Jeff's session ended)
        // Card #1000: modified 2026-10-10T12:00:00Z (BEFORE Jeff's session ended -> MUST NOT appear in changes)
        // Card #1: no timestamp (Athena collection item shape -> MUST NOT appear in changes)
        if (url.startsWith('/fact-cards')) {
          cardsQueryUrl = url;
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({
            data: [
              {
                id: 'chorus:4536',
                data: {
                  label: 'werk-test verdict + report',
                  status: 'WIP',
                  modified: '2026-10-15T02:00:00Z',
                },
                links: { assignee: 'chorus:role-kade' },
              },
              {
                id: 'chorus:4530',
                data: {
                  label: 'Clearing v2 on Buzz + Pulse',
                  status: 'Next',
                  modified: '2026-10-15T01:00:00Z',
                },
                links: { assignee: 'chorus:role-abby-normal' },
              },
              {
                id: 'chorus:1000',
                data: {
                  label: 'ICD migration fidelity gate',
                  status: 'Done',
                  modified: '2026-10-10T12:00:00Z',
                },
                links: { assignee: 'chorus:role-silas' },
              },
              {
                id: 'chorus:1',
                data: {
                  label: 'Vision refinement session',
                  status: 'Done',
                },
                links: { assignee: 'chorus:role-wren' },
              },
            ],
          }));
          return;
        }

        // 5. Stub Board WIP and Next (must NOT be called for Today changed cards)
        if (url.startsWith('/fact-board/wip')) {
          boardWipCalls++;
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({
            data: {
              cards: [
                { id: 4536, owner: 'Kade', title: 'werk-test verdict + report' },
                { id: 4530, owner: 'Abby-normal', title: 'Clearing v2 on Buzz + Pulse' },
              ],
            },
          }));
          return;
        }
        if (url.startsWith('/fact-board/next')) {
          boardNextCalls++;
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({
            data: {
              cards: [
                { id: 3198, owner: 'Silas', title: 'Nightly lies' },
              ],
            },
          }));
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

  test('1. NEGATIVE PROOF: Who is working reuses Roles (only active working roles appear in Working now)', async () => {
    const appInstance = createApp({
      relayUrl: 'ws://127.0.0.1:3499',
      rolesUrl: `http://127.0.0.1:${stubPort}/fact-roles`,
      sessionsUrl: `http://127.0.0.1:${stubPort}/fact-sessions`,
      lokiUrl: `http://127.0.0.1:${stubPort}/fact-loki`,
      boardUrl: `http://127.0.0.1:${stubPort}/fact-board`,
    });
    const s = http.createServer(appInstance.app);
    await new Promise<void>((r) => s.listen(0, '127.0.0.1', () => r()));
    const p = (s.address() as any).port;

    try {
      const res = await fetch(`http://127.0.0.1:${p}/today`, {
        headers: {
          'x-roles-url': `http://127.0.0.1:${stubPort}/fact-roles`,
          'x-sessions-url': `http://127.0.0.1:${stubPort}/fact-sessions`,
          'x-loki-url': `http://127.0.0.1:${stubPort}/fact-loki`,
          'x-board-url': `http://127.0.0.1:${stubPort}/fact-board`,
        },
      });
      expect(res.status).toBe(200);
      const html = await res.text();

      // Working now section must exist
      expect(html).toContain('Working now');

      // Kade is the ONLY role in working state (has open session and count5m > 0)
      const workingSnippet = html.slice(html.indexOf('Working now'));
      expect(workingSnippet).toContain('Kade');
      expect(workingSnippet).toContain('#4536');
      expect(workingSnippet).toContain('werk-test verdict + report');

      // NEGATIVE PROOF: Wren is idle (count5m == 0) and Silas is off; neither is working
      // So they MUST NOT be listed under 'Working now' as 'working'
      expect(workingSnippet).not.toContain('Wren · <span class="st"><span class="dot d-ok"></span>working</span>');
      expect(workingSnippet).not.toContain('Silas · <span class="st"><span class="dot d-ok"></span>working</span>');
    } finally {
      await new Promise<void>((r) => s.close(() => r()));
    }
  });

  test('2. NEGATIVE PROOF: Alerts render dynamically with resolving card links (#3734)', async () => {
    // 2A: Alerts door returns dynamic unseen alert instances
    const appInstance = createApp({
      relayUrl: 'ws://127.0.0.1:3499',
      rolesUrl: `http://127.0.0.1:${stubPort}/fact-roles`,
      sessionsUrl: `http://127.0.0.1:${stubPort}/fact-sessions`,
      lokiUrl: `http://127.0.0.1:${stubPort}/fact-loki`,
      boardUrl: `http://127.0.0.1:${stubPort}/fact-board`,
      cardsUrl: `http://127.0.0.1:${stubPort}/fact-cards`,
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
      rolesUrl: `http://127.0.0.1:${stubPort}/fact-roles`,
      sessionsUrl: `http://127.0.0.1:${stubPort}/fact-sessions`,
      lokiUrl: `http://127.0.0.1:${stubPort}/fact-loki`,
      boardUrl: `http://127.0.0.1:${stubPort}/fact-board`,
      cardsUrl: `http://127.0.0.1:${stubPort}/fact-cards`,
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

  test("3. NEGATIVE PROOF: Changed overnight diffs cards modified strictly after Jeff's last session ended", async () => {
    boardWipCalls = 0;
    boardNextCalls = 0;
    cardsQueryUrl = null;

    const appInstance = createApp({
      relayUrl: 'ws://127.0.0.1:3499',
      cardsUrl: `http://127.0.0.1:${stubPort}/fact-cards`,
      sessionsUrl: `http://127.0.0.1:${stubPort}/fact-sessions`,
      rolesUrl: `http://127.0.0.1:${stubPort}/fact-roles`,
      lokiUrl: `http://127.0.0.1:${stubPort}/fact-loki`,
      boardUrl: `http://127.0.0.1:${stubPort}/fact-board`,
    });
    const s = http.createServer(appInstance.app);
    await new Promise<void>((r) => s.listen(0, '127.0.0.1', () => r()));
    const p = (s.address() as any).port;

    try {
      const res = await fetch(`http://127.0.0.1:${p}/today`, {
        headers: {
          'x-cards-url': `http://127.0.0.1:${stubPort}/fact-cards`,
          'x-sessions-url': `http://127.0.0.1:${stubPort}/fact-sessions`,
        },
      });
      expect(res.status).toBe(200);
      const html = await res.text();

      expect(html).toContain('Changed overnight');

      // NEGATIVE PROOF: cards door must be queried with a since-filter derived from Jeff's session watermark
      expect(cardsQueryUrl).toBeDefined();
      expect(cardsQueryUrl).toContain('since=');
      expect(cardsQueryUrl).toContain('2026-10-14T20');

      // #4536 and #4530 were modified after Jeff's session ended (2026-10-14T20:00:00Z)
      expect(html).toContain('#4536');
      expect(html).toContain('#4530');

      // NEGATIVE PROOF: #1000 was modified before Jeff's session ended (2026-10-10) -> MUST NOT appear
      const changedSnippet = html.slice(html.indexOf('Changed overnight'), html.indexOf('Alerts · new or changed only'));
      expect(changedSnippet).not.toContain('#1000');

      // NEGATIVE PROOF: Card #1 has no modified timestamp -> MUST NOT appear in Changed overnight
      expect(changedSnippet).not.toContain('#1</a>');

      // NEGATIVE PROOF: /next fallback was completely removed
      expect(boardNextCalls).toBe(0);
    } finally {
      await new Promise<void>((r) => s.close(() => r()));
    }
  });

  test('4. NEGATIVE PROOF: Decisions owed stays red by name (must read no source (part 2))', async () => {
    const appInstance = createApp({
      relayUrl: 'ws://127.0.0.1:3499',
      rolesUrl: `http://127.0.0.1:${stubPort}/fact-roles`,
      sessionsUrl: `http://127.0.0.1:${stubPort}/fact-sessions`,
      lokiUrl: `http://127.0.0.1:${stubPort}/fact-loki`,
      boardUrl: `http://127.0.0.1:${stubPort}/fact-board`,
      cardsUrl: `http://127.0.0.1:${stubPort}/fact-cards`,
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
  test('5. NEGATIVE PROOF: an unreadable roles door names itself in Working now, never "0 roles working now"', async () => {
    // Measured 2026-10-16 05:16: the first Today load under a busy box said
    // "0 roles working now" while the Roles page showed Kade and Wren working.
    // A read that failed must not render as a measured zero.
    const appInstance = createApp({
      relayUrl: 'ws://127.0.0.1:3499',
      rolesUrl: `http://127.0.0.1:${stubPort}/no-such-roles-door`,
      sessionsUrl: `http://127.0.0.1:${stubPort}/fact-sessions`,
      lokiUrl: `http://127.0.0.1:${stubPort}/fact-loki`,
      boardUrl: `http://127.0.0.1:${stubPort}/fact-board`,
    });
    const s = http.createServer(appInstance.app);
    await new Promise<void>((r) => s.listen(0, '127.0.0.1', () => r()));
    const p = (s.address() as any).port;
    try {
      const html = await (await fetch(`http://127.0.0.1:${p}/today`)).text();
      const working = html.slice(html.indexOf('working-now'));
      expect(working).toContain('unmeasured: roles unreadable');
      expect(working).not.toContain('0 roles working now');
    } finally {
      await new Promise<void>((r) => s.close(() => r()));
    }
  });
  test.each([
    ['activity (Loki)', { lokiUrl: '/no-such-loki' }, 'activity unreadable'],
    ['open sessions', { sessionsUrl: '/no-such-sessions' }, 'sessions unreadable'],
  ])('6. NEGATIVE PROOF: unreadable %s names itself in Working now, never "0 roles working now"', async (_name, broken, expected) => {
    // Measured 2026-10-16 06:41 on the landed build: a cold Today load said
    // "0 roles working now" with the roles door live; every role had read
    // unmeasured or off because a source behind it timed out.
    const urls: Record<string, string> = {
      rolesUrl: '/fact-roles', sessionsUrl: '/fact-sessions', lokiUrl: '/fact-loki', boardUrl: '/fact-board', ...broken,
    };
    const base = `http://127.0.0.1:${stubPort}`;
    const appInstance = createApp({
      relayUrl: 'ws://127.0.0.1:3499',
      rolesUrl: base + urls.rolesUrl,
      sessionsUrl: base + urls.sessionsUrl,
      lokiUrl: base + urls.lokiUrl,
      boardUrl: base + urls.boardUrl,
    });
    const s = http.createServer(appInstance.app);
    await new Promise<void>((r) => s.listen(0, '127.0.0.1', () => r()));
    const p = (s.address() as any).port;
    try {
      const html = await (await fetch(`http://127.0.0.1:${p}/today`)).text();
      const working = html.slice(html.indexOf('working-now'));
      expect(working).toContain(expected);
      expect(working).not.toContain('0 roles working now');
    } finally {
      await new Promise<void>((r) => s.close(() => r()));
    }
  });
});
