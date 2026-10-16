// @test-type: unit — verifies the trust strip presents pulse /api/readings as sent (as-of, lag, unmeasured) plus v2's own relay chip (#4563)
// @card: #4530
// @owner: abby-normal
// @domain: messages

import http from 'http';
import { DataService } from '../src/data-service';
import { BuzzRelayClient } from '../src/relay-client';
import { MessageRouter } from '../src/router';
import { renderPageShell } from '../src/views';

describe('Trust Strip: pulse readings as sent, and the relay chip', () => {
  let stubServer: http.Server;
  let stubPort: number;
  let relay: BuzzRelayClient;
  let router: MessageRouter;
  let dataService: DataService;

  beforeAll(async () => {
    await new Promise<void>((resolve) => {
      stubServer = http.createServer((req, res) => {
        if (req.url?.startsWith('/dup/api/readings')) {
          // live shape 10-16 12:13: pulse lists an unreadable source as a reading AND in unmeasured,
          // and reads 'rooms' itself; the page showed rooms three times and decisions twice
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({
            derivedAt: '2026-10-16T16:12:50Z',
            readings: [
              { source: 'board', observedAt: '2026-10-16T16:12:50Z', status: 'ok', asOf: 'as of 12:13' },
              { source: 'rooms', observedAt: '2026-10-16T16:12:50Z', status: 'unmeasured', reason: 'relay not configured' },
              { source: 'decisions', observedAt: '2026-10-16T16:12:50Z', status: 'unmeasured', reason: 'no source, part 2' },
            ],
            unmeasured: [
              { source: 'rooms', reason: 'relay not configured' },
              { source: 'decisions', reason: 'no source, part 2' },
              { source: 'loki', reason: 'timeout' },
            ],
          }));
          return;
        }
        if (req.url?.startsWith('/pulse/api/readings')) {
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({
            derivedAt: '2026-10-16T14:00:00Z',
            readings: [
              { source: 'board', observedAt: '2026-10-16T14:00:00Z', status: 'ok', asOf: 'as of 09:59' },
              { source: 'roles', observedAt: '2026-10-16T14:00:00Z', status: 'ok', asOf: 'as of 09:26', lagMinutes: 34 },
              { source: 'decisions', observedAt: '2026-10-16T14:00:00Z', status: 'unmeasured', reason: 'no source, part 2' },
            ],
            unmeasured: [],
          }));
          return;
        }
        res.writeHead(200).end();
      });
      stubServer.listen(0, '127.0.0.1', () => {
        stubPort = (stubServer.address() as any).port;
        resolve();
      });
    });
  });

  afterAll(async () => {
    await new Promise<void>((resolve) => stubServer.close(() => resolve()));
  });

  beforeEach(() => {
    relay = new BuzzRelayClient({ relayUrl: 'ws://127.0.0.1:3499' });
    (relay as any).connected = true;
    router = new MessageRouter();
    dataService = new DataService({
      pulseUrl: `http://127.0.0.1:${stubPort}/pulse`,
      relay,
      router,
    });
  });

  test('1. Every page renders a chip per pulse reading, with the as-of pulse sent', async () => {
    const sources = await dataService.getTrustStripSources();
    expect(sources.map(s => s.source)).toEqual(['rooms', 'board', 'roles', 'decisions']);
    const html = renderPageShell('Test', 'rooms', '<div>Content</div>', sources);
    expect(html).toContain('class="strip trust-strip"');
    expect(html).toContain('board · as of 09:59');
  });

  test('2. NEGATIVE PROOF: an unmeasured reading shows red by name with its reason, never a time', async () => {
    const sources = await dataService.getTrustStripSources();
    const decisions = sources.find(s => s.source === 'decisions');
    expect(decisions?.status).toBe('unmeasured');
    expect(decisions?.reason).toBe('no source, part 2');
    expect(decisions?.asOf).toBeUndefined();
    const html = renderPageShell('Test', 'rooms', '<div>Content</div>', sources);
    expect(html).toContain('<span class="chip strip-source-fail"><i class="fail"></i><span class="fail-source-name">decisions</span> unmeasured (no source, part 2)</span>');
    expect(html).not.toContain('<span class="fail-source-name">board</span>');
    expect(html).not.toContain('<span class="fail-source-name">roles</span>');
  });

  test('3. NEGATIVE PROOF: pulse lag shows with the neutral grey chip, never red', async () => {
    const sources = await dataService.getTrustStripSources();
    expect(sources.find(s => s.source === 'roles')?.lagMinutes).toBe(34);
    const html = renderPageShell('Test', 'rooms', '<div>Content</div>', sources);
    expect(html).toContain('strip-lag-neutral');
    expect(html).toContain('roles · as of 09:26 (34m)');
    expect(html).not.toContain('<span class="fail-source-name">roles</span>');
  });

  test('4. NEGATIVE PROOF: pulse unreachable reads one unmeasured chip with the reason, never an empty strip', async () => {
    const sources = await dataService.getTrustStripSources({ pulseUrl: 'http://127.0.0.1:1' });
    const readings = sources.find(s => s.source === 'readings');
    expect(readings?.status).toBe('unmeasured');
    expect(readings?.reason).toMatch(/\/api\/readings unreadable/);
  });

  test('5. NEGATIVE PROOF: without configured relay URL, trust strip shows rooms red by name ("relay not configured")', async () => {
    const unconfiguredRelay = new BuzzRelayClient({ relayUrl: '' });
    const ds = new DataService({
      pulseUrl: `http://127.0.0.1:${stubPort}/pulse`,
      relay: unconfiguredRelay,
      router,
      relayConfigured: false,
    });

    const sources = await ds.getTrustStripSources();
    const roomsSource = sources.find(s => s.source === 'rooms');
    expect(roomsSource).toBeDefined();
    expect(roomsSource?.status).toBe('failed');
    expect(roomsSource?.reason).toBe('relay not configured');

    const html = renderPageShell('Test', 'rooms', '<div>Content</div>', sources);
    expect(html).toContain('<span class="chip strip-source-fail"><i class="fail"></i><span class="fail-source-name">rooms</span> failed (relay not configured)</span>');
  });

  test('6. NEGATIVE PROOF: when relay is not connected, trust strip shows rooms red by name ("relay disconnected")', async () => {
    (relay as any).connected = false;
    expect(relay.isConnected()).toBe(false);

    const sources = await dataService.getTrustStripSources();
    const roomsSource = sources.find(s => s.source === 'rooms');
    expect(roomsSource).toBeDefined();
    expect(roomsSource?.status).toBe('failed');
    expect(roomsSource?.reason).toBe('relay disconnected');

    const html = renderPageShell('Test', 'rooms', '<div>Content</div>', sources);
    expect(html).toContain('<span class="chip strip-source-fail"><i class="fail"></i><span class="fail-source-name">rooms</span> failed (relay disconnected)</span>');
  });
  test('one chip per source: no source shows twice, and an unmeasured entry with no reading still shows', async () => {
    const ds = new DataService({ pulseUrl: `http://127.0.0.1:${stubPort}/dup`, relay, router });
    const sources = await ds.getTrustStripSources();
    const names = sources.map((s) => s.source);
    expect(names.filter((n) => n === 'rooms')).toHaveLength(1);
    expect(names.filter((n) => n === 'decisions')).toHaveLength(1);
    // loki has no reading, only an unmeasured entry: it must still be named
    expect(names).toContain('loki');
    // rooms is v2's own relay chip (v2 is connected in this test), not pulse's view of it
    expect(sources.find((s) => s.source === 'rooms')?.status).toBe('ok');
  });
});
