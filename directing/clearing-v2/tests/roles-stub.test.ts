// @test-type: unit — verifies dynamic roles rendering against unseen stub source and negative proof (#3734)
// @card: #4530
// @owner: abby-normal
// @domain: messages

import http from 'http';
import { DataService } from '../src/data-service';
import { renderRolesPage } from '../src/views';
import { BuzzRelayClient } from '../src/relay-client';
import { MessageRouter } from '../src/router';

describe('Roles Dynamic Source & Negative Proof Contract (#3734)', () => {
  let stubServer: http.Server;
  let stubPort: number;

  beforeAll(async () => {
    await new Promise<void>((resolve) => {
      stubServer = http.createServer((req, res) => {
      if (req.url?.startsWith('/fail')) {
        res.writeHead(500, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: 'Door Server Error' }));
        return;
      }
      if (req.url?.startsWith('/fact-roles')) {
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({
          data: [
            { name: 'p-working', label: 'Principal Working', roleKind: 'agent' },
            { name: 'p-stale', label: 'Principal Stale', roleKind: 'agent' },
            { name: 'p-off', label: 'Principal Off', roleKind: 'agent' },
          ],
        }));
        return;
      }
      if (req.url?.startsWith('/fact-sessions')) {
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({
          data: [
            { actsAs: 'role-p-working', sessionState: 'open' },
            { actsAs: 'role-p-stale', sessionState: 'open' },
          ],
        }));
        return;
      }
      if (req.url?.startsWith('/fact-loki/ready') || req.url?.startsWith('/loki-api/ready') || req.url?.startsWith('/loki-25m/ready')) {
        res.writeHead(200, { 'Content-Type': 'text/plain' });
        res.end('ready');
        return;
      }
      if (req.url?.startsWith('/fact-loki/loki/api/v1/query')) {
        const urlStr = req.url || '';
        const is5m = urlStr.includes('%5B5m%5D') || urlStr.includes('[5m]');
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({
          status: 'success',
          data: {
            resultType: 'vector',
            result: [
              {
                metric: { role: 'p-working' },
                value: [Math.floor(Date.now() / 1000), is5m ? '3' : '15'],
              },
            ],
          },
        }));
        return;
      }
      if (req.url?.startsWith('/loki-25m-roles')) {
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({
          data: [
            { name: 'p-25m-ago', label: 'Principal Twenty Five Min', roleKind: 'agent' },
          ],
        }));
        return;
      }
      if (req.url?.startsWith('/loki-25m-sessions')) {
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({
          data: [
            { actsAs: 'role-p-25m-ago', sessionState: 'open' },
          ],
        }));
        return;
      }
      if (req.url?.startsWith('/loki-25m/loki/api/v1/query')) {
        const urlStr = req.url || '';
        const is5m = urlStr.includes('%5B5m%5D') || urlStr.includes('[5m]');
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({
          status: 'success',
          data: {
            resultType: 'vector',
            result: is5m
              ? [] // 0 actions in last 5m
              : [
                  {
                    metric: { role: 'p-25m-ago' },
                    value: [Math.floor(Date.now() / 1000), '4'], // 4 actions in last 30m!
                  },
                ],
          },
        }));
        return;
      }
      if (req.url?.startsWith('/loki-down-roles')) {
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({
          data: [
            { name: 'p-online', label: 'Principal Online', roleKind: 'agent' },
          ],
        }));
        return;
      }
      if (req.url?.startsWith('/loki-down-sessions')) {
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({
          data: [
            { actsAs: 'role-p-online', sessionState: 'open' },
          ],
        }));
        return;
      }
      if (req.url?.startsWith('/loki-down')) {
        res.writeHead(500, { 'Content-Type': 'text/plain' });
        res.end('door unreadable (500)');
        return;
      }
      if (req.url?.startsWith('/empty-sessions')) {
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ data: [] }));
        return;
      }
      if (req.url?.startsWith('/roles')) {
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({
          data: [
            { name: 'zz-test-pilot', label: 'ZZ Test Pilot', roleKind: 'agent' },
            { name: 'zz-test-person', label: 'ZZ Test Person', roleKind: 'human' },
            { name: 'crawler', label: 'Crawler Service', roleKind: 'service' },
            { name: 'zz-probe-old', label: 'Old Probe', roleKind: 'agent' },
          ],
        }));
        return;
      }
      if (req.url?.startsWith('/board/wip')) {
        const url = new URL(req.url, 'http://localhost');
        const role = url.searchParams.get('role');
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
      const addr = stubServer.address() as any;
      stubPort = addr.port;
      resolve();
    });
    });
  });

  afterAll(async () => {
    await new Promise<void>((resolve) => {
      stubServer.close(() => resolve());
    });
  });

  test('1. Dynamically renders unseen roles and WIP cards, excluding service/crawler/probes', async () => {
    const relay = new BuzzRelayClient({ relayUrl: 'ws://127.0.0.1:3499' });
    const router = new MessageRouter();
    const ds = new DataService({
      rolesUrl: `http://127.0.0.1:${stubPort}/roles`,
      boardUrl: `http://127.0.0.1:${stubPort}/board`,
      relay,
      router,
    });

    const rolesData = await ds.getRolesData();
    const sources = await ds.getTrustStripSources({
      rolesUrl: `http://127.0.0.1:${stubPort}/roles`,
      boardUrl: `http://127.0.0.1:${stubPort}/board`,
    });

    const html = renderRolesPage(rolesData, sources);

    // Shows unseen roles by name
    expect(html).toContain('ZZ Test Pilot');
    expect(html).toContain('ZZ Test Person');

    // Shows their cards from WIP
    expect(html).toContain('#9901');
    expect(html).toContain('Test Pilot Flight');
    expect(html).toContain('#9902');
    expect(html).toContain('Ground Control Ops');

    // Excludes crawler (roleKind service) and probe rows
    expect(html).not.toContain('Crawler Service');
    expect(html).not.toContain('crawler');
    expect(html).not.toContain('zz-probe-old');
    expect(html).not.toContain('Old Probe');

    // Trust strip is healthy
    const rolesSource = sources.find(s => s.source === 'roles');
    expect(rolesSource?.status).toBe('ok');
  });

  test('2. NEGATIVE PROOF: stub returns 500 -> trust strip names roles failed, never a list', async () => {
    const relay = new BuzzRelayClient({ relayUrl: 'ws://127.0.0.1:3499' });
    const router = new MessageRouter();
    const ds = new DataService({
      rolesUrl: `http://127.0.0.1:${stubPort}/fail`,
      boardUrl: `http://127.0.0.1:${stubPort}/board`,
      relay,
      router,
    });

    const rolesData = await ds.getRolesData();
    const sources = await ds.getTrustStripSources({
      rolesUrl: `http://127.0.0.1:${stubPort}/fail`,
      boardUrl: `http://127.0.0.1:${stubPort}/board`,
    });

    const html = renderRolesPage(rolesData, sources);

    // 0 principals rendered, NEVER a list
    expect(rolesData.principals.length).toBe(0);
    expect(html).toContain('0 principals found');
    expect(html).not.toContain('<table class="roles-table">');

    // Trust strip chip highlights failing source by name
    expect(html).toContain('strip-source-fail');
    expect(html).toContain('fail-source-name">roles</span> failed (door unreadable (500))');
  });

  test('3. Roles 2: Fact-based state derivation from Loki metric counts and sessions (working, stale, off) and unmodeled negative proof (#4361)', async () => {
    const relay = new BuzzRelayClient({ relayUrl: 'ws://127.0.0.1:3499' });
    const router = new MessageRouter();

    const ds = new DataService({
      rolesUrl: `http://127.0.0.1:${stubPort}/fact-roles`,
      boardUrl: `http://127.0.0.1:${stubPort}/board`,
      sessionsUrl: `http://127.0.0.1:${stubPort}/fact-sessions`,
      lokiUrl: `http://127.0.0.1:${stubPort}/fact-loki`,
      relay,
      router,
    });

    const rolesData = await ds.getRolesData();
    const sources = await ds.getTrustStripSources({
      rolesUrl: `http://127.0.0.1:${stubPort}/fact-roles`,
      boardUrl: `http://127.0.0.1:${stubPort}/board`,
      lokiUrl: `http://127.0.0.1:${stubPort}/fact-loki`,
    });

    const html = renderRolesPage(rolesData, sources);

    // 1. Fact-derived working state from Loki count in [5m] > 0
    const workingPrincipal = rolesData.principals.find(p => p.id === 'p-working');
    expect(workingPrincipal?.state).toBe('working');
    expect(html).toContain('working');

    // 2. Fact-derived stale state: count in [30m] == 0, displayed as "stale (no action in 30m)" with no number literals
    const stalePrincipal = rolesData.principals.find(p => p.id === 'p-stale');
    expect(stalePrincipal?.state).toBe('stale');
    expect(html).toContain('stale (no action in 30m)');

    // 3. Fact-derived off state from missing session in /v1/identity/sessions
    const offPrincipal = rolesData.principals.find(p => p.id === 'p-off');
    expect(offPrincipal?.state).toBe('off');
    expect(html).toContain('off');

    // 4. NEGATIVE PROOF: unmodeled states have no source in presence facts (#4361) and are NEVER faked
    expect(rolesData.principals.find(p => p.state === 'waiting on you')).toBeUndefined();
    expect(rolesData.principals.find(p => p.state === 'blocked')).toBeUndefined();
    expect(html).not.toContain('waiting on you');
    expect(html).not.toContain('Decisions · hold');
  });

  test('4. NEGATIVE PROOF (#3734): empty sessions marks all roles off (never online or working)', async () => {
    const relay = new BuzzRelayClient({ relayUrl: 'ws://127.0.0.1:3499' });
    const router = new MessageRouter();

    const dsEmptySessions = new DataService({
      rolesUrl: `http://127.0.0.1:${stubPort}/fact-roles`,
      boardUrl: `http://127.0.0.1:${stubPort}/board`,
      sessionsUrl: `http://127.0.0.1:${stubPort}/empty-sessions`,
      lokiUrl: `http://127.0.0.1:${stubPort}/fact-loki`,
      relay,
      router,
    });
    const rolesEmptySessions = await dsEmptySessions.getRolesData();
    expect(rolesEmptySessions.principals.every(p => p.state === 'off')).toBe(true);
    expect(rolesEmptySessions.principals.some(p => p.state === 'working')).toBe(false);
  });

  test('5. WREN 11:43 RED 1: role active 25m ago must not read stale under 30m metric count query', async () => {
    const relay = new BuzzRelayClient({ relayUrl: 'ws://127.0.0.1:3499' });
    const router = new MessageRouter();

    // A. RED PROOF: an empty query returns count=0 -> state becomes 'stale'
    const dsTruncated = new DataService({
      rolesUrl: `http://127.0.0.1:${stubPort}/loki-25m-roles`,
      boardUrl: `http://127.0.0.1:${stubPort}/board`,
      sessionsUrl: `http://127.0.0.1:${stubPort}/loki-25m-sessions`,
      lokiUrl: `http://127.0.0.1:${stubPort}/fact-loki`, // returns 0 results for p-25m-ago
      relay,
      router,
    });
    const rolesTruncated = await dsTruncated.getRolesData();
    const pTruncated = rolesTruncated.principals.find(p => p.id === 'p-25m-ago');
    expect(pTruncated?.state).toBe('stale');

    // B. GREEN PROOF: sum by (role) (count_over_time(... [30m])) returns count > 0 for p-25m-ago,
    // so it is NOT silent >= 30m and does NOT read stale (it reads idle)
    const dsWithCounts = new DataService({
      rolesUrl: `http://127.0.0.1:${stubPort}/loki-25m-roles`,
      boardUrl: `http://127.0.0.1:${stubPort}/board`,
      sessionsUrl: `http://127.0.0.1:${stubPort}/loki-25m-sessions`,
      lokiUrl: `http://127.0.0.1:${stubPort}/loki-25m`,
      relay,
      router,
    });
    const rolesWithCounts = await dsWithCounts.getRolesData();
    const pWithCounts = rolesWithCounts.principals.find(p => p.id === 'p-25m-ago');
    expect(pWithCounts?.state).not.toBe('stale');
    expect(pWithCounts?.state).toBe('idle');

    const sources = await dsWithCounts.getTrustStripSources({
      rolesUrl: `http://127.0.0.1:${stubPort}/loki-25m-roles`,
      boardUrl: `http://127.0.0.1:${stubPort}/board`,
      lokiUrl: `http://127.0.0.1:${stubPort}/loki-25m`,
    });
    const html = renderRolesPage(rolesWithCounts, sources);
    expect(html).toContain('idle');
    expect(html).not.toContain('stale');
  });

  test('6. WREN 11:43 RED 2: Loki-down must read unmeasured instead of defaulting to stale/off', async () => {
    const relay = new BuzzRelayClient({ relayUrl: 'ws://127.0.0.1:3499' });
    const router = new MessageRouter();

    // With Loki returning 500 error:
    const dsLokiDown = new DataService({
      rolesUrl: `http://127.0.0.1:${stubPort}/loki-down-roles`,
      boardUrl: `http://127.0.0.1:${stubPort}/board`,
      sessionsUrl: `http://127.0.0.1:${stubPort}/loki-down-sessions`,
      lokiUrl: `http://127.0.0.1:${stubPort}/loki-down`,
      relay,
      router,
    });

    const rolesData = await dsLokiDown.getRolesData();
    const pOnline = rolesData.principals.find(p => p.id === 'p-online');

    // RED PROOF: previously catch {} turned Loki failure into stale or off.
    // GREEN PROOF: role with open session must read unmeasured
    expect(pOnline?.state).not.toBe('stale');
    expect(pOnline?.state).not.toBe('off');
    expect(pOnline?.state).toBe('unmeasured');

    const sources = await dsLokiDown.getTrustStripSources({
      rolesUrl: `http://127.0.0.1:${stubPort}/loki-down-roles`,
      boardUrl: `http://127.0.0.1:${stubPort}/board`,
      lokiUrl: `http://127.0.0.1:${stubPort}/loki-down`,
    });

    const html = renderRolesPage(rolesData, sources);
    expect(html).toContain('unmeasured');
    expect(html).toContain('strip-source-fail');
    expect(html).toContain('fail-source-name">loki</span> failed');
  });
});
