// @test-type: unit — verifies trust strip freshness, measured elapsed lag, and failure isolation
// @card: #4530
// @owner: abby-normal
// @domain: messages

import http from 'http';
import { DataService } from '../src/data-service';
import { BuzzRelayClient } from '../src/relay-client';
import { MessageRouter } from '../src/router';
import { renderPageShell } from '../src/views';

describe('Trust Strip: Freshness, Measured Elapsed Lag, and Failure Isolation', () => {
  let stubServer: http.Server;
  let stubPort: number;
  let relay: BuzzRelayClient;
  let router: MessageRouter;
  let dataService: DataService;

  beforeAll(async () => {
    await new Promise<void>((resolve) => {
      stubServer = http.createServer((req, res) => {
        if (req.url?.startsWith('/board/wip')) {
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ timestamp: new Date().toISOString(), data: { cards: [] } }));
          return;
        }
        if (req.url?.startsWith('/roles')) {
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ data: [] }));
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
      rolesUrl: `http://127.0.0.1:${stubPort}/roles`,
      boardUrl: `http://127.0.0.1:${stubPort}/board`,
      relay,
      router,
    });
  });

  test('1. Every page renders trust strip with chips for all core sources', async () => {
    const sources = await dataService.getTrustStripSources();
    const html = renderPageShell('Test', 'rooms', '<div>Content</div>', sources);
    expect(html).toContain('class="strip trust-strip"');
    expect(html).toContain('board');
    expect(html).toContain('rooms');
    expect(html).toContain('decisions');
    expect(html).toContain('roles');
  });

  test('2. NEGATIVE PROOF: decisions has no source in Part 1 and displays red by name ("no source, part 2"), never a literal time', async () => {
    const sources = await dataService.getTrustStripSources();
    const decisionsSource = sources.find(s => s.source === 'decisions');
    expect(decisionsSource).toBeDefined();
    expect(decisionsSource?.status).toBe('failed');
    expect(decisionsSource?.reason).toBe('no source, part 2');
    expect(decisionsSource?.asOf).not.toContain('10:04');

    const html = renderPageShell('Test', 'rooms', '<div>Content</div>', sources);
    expect(html).toContain('<span class="chip strip-source-fail"><i class="fail"></i><span class="fail-source-name">decisions</span> failed (no source, part 2)</span>');
    expect(html).not.toContain('10:04');
  });

  test('3. NEGATIVE PROOF: when a named source fails, ONLY that source is highlighted red', async () => {
    const sources = await dataService.getTrustStripSources({ failSource: 'decisions' });
    const html = renderPageShell('Test', 'rooms', '<div>Content</div>', sources);

    // Failing source is red and named
    expect(html).toContain('<span class="chip strip-source-fail"><i class="fail"></i><span class="fail-source-name">decisions</span> failed');
    // Other sources remain non-failing
    expect(html).not.toContain('<span class="fail-source-name">board</span> failed');
    expect(html).not.toContain('<span class="fail-source-name">rooms</span> failed');
    expect(html).not.toContain('<span class="fail-source-name">roles</span> failed');
  });

  test('4. NEGATIVE PROOF: lag is displayed with neutral grey indicator, NEVER red failure', async () => {
    const sources = await dataService.getTrustStripSources({ lagSource: 'roles' });
    const rolesSource = sources.find(s => s.source === 'roles');
    expect(rolesSource).toBeDefined();
    expect(rolesSource?.lagMinutes).toBe(34);

    const html = renderPageShell('Test', 'rooms', '<div>Content</div>', sources);
    // Must have neutral lag chip
    expect(html).toContain('strip-lag-neutral');
    expect(html).toContain('34m');
    // Must NOT have failure chip for roles
    expect(html).not.toContain('<span class="fail-source-name">roles</span> failed');
  });

  test('5. NEGATIVE PROOF: without configured relay URL, trust strip shows rooms red by name ("relay not configured")', async () => {
    const unconfiguredRelay = new BuzzRelayClient({ relayUrl: '' });
    const ds = new DataService({
      rolesUrl: `http://127.0.0.1:${stubPort}/roles`,
      boardUrl: `http://127.0.0.1:${stubPort}/board`,
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
});
