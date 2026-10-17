// @test-type: unit — tests for Pulse source readings derivation, service cache, and endpoint
// @card: #4563
// @domain: awareness
// @owner: wren

import http from 'http';
import request from 'supertest';
import { MessageStore } from './store';
import { createApp } from './service';
import {
  deriveReadings,
  ReadingsService,
  ReadingsFactInput,
  RawProbeResult,
  buildLiveReadingsFetcher,
} from './source-readings';

const SRC_BOARD = 'board';
const SRC_ROOMS = 'rooms';
const SRC_DECISIONS = 'decisions';
const SRC_ROLES = 'roles';
const SRC_LOKI = 'loki';
const REASON_NO_SOURCE = 'no source, part 2';
const REASON_CONN_REFUSED = 'connection refused';
const STATUS_OK = 'ok';
const STATUS_FAILED = 'failed';
const STATUS_UNMEASURED = 'unmeasured';

const baseNow = new Date('2026-10-16T11:20:00Z');
const BOARD_TIME = '2026-10-16T11:18:00Z';

describe('Source Readings — Derivation', () => {
  test('returns 5 canonical sources with decisions as honest unmeasured', () => {
    const probes: RawProbeResult[] = [
      { source: SRC_BOARD, ok: true, timestamp: BOARD_TIME },
      { source: SRC_ROOMS, ok: true },
      { source: SRC_DECISIONS, ok: false, error: REASON_NO_SOURCE },
      { source: SRC_ROLES, ok: true },
      { source: SRC_LOKI, ok: true },
    ];

    const input: ReadingsFactInput = { now: baseNow, probes };
    const res = deriveReadings(input);

    expect(res.derivedAt).toBe(baseNow.toISOString());
    expect(res.readings.length).toBe(5);

    const board = res.readings.find((r) => r.source === SRC_BOARD);
    expect(board?.status).toBe(STATUS_OK);
    expect(board?.lastWrite).toBe(BOARD_TIME);
    expect(board?.observedAt).toBe(baseNow.toISOString());
    expect(board?.asOf).toMatch(/as of \d{2}:\d{2}/);

    const decisions = res.readings.find((r) => r.source === SRC_DECISIONS);
    expect(decisions?.status).toBe(STATUS_UNMEASURED);
    expect(decisions?.reason).toBe(REASON_NO_SOURCE);

    const unmeasuredDecisions = res.unmeasured.find((u) => u.source === SRC_DECISIONS);
    expect(unmeasuredDecisions?.reason).toBe(REASON_NO_SOURCE);
    expect(res.unmeasured.length).toBe(1);
  });

  test('reports lagMinutes and formatted asOf for lagged sources', () => {
    const probes: RawProbeResult[] = [
      { source: SRC_ROLES, ok: true, lagMinutes: 34 },
    ];

    const input: ReadingsFactInput = { now: baseNow, probes };
    const res = deriveReadings(input);

    const roles = res.readings.find((r) => r.source === SRC_ROLES);
    expect(roles?.status).toBe(STATUS_OK);
    expect(roles?.lagMinutes).toBe(34);
    expect(roles?.asOf).toContain('(34m)');
  });
});

describe('Source Readings — Negative Proofs', () => {
  test('NEGATIVE PROOF: failed source is isolated and recorded in unmeasured', () => {
    const probes: RawProbeResult[] = [
      { source: SRC_BOARD, ok: false, error: REASON_CONN_REFUSED },
      { source: SRC_ROOMS, ok: true },
      { source: SRC_DECISIONS, ok: false, error: REASON_NO_SOURCE },
      { source: SRC_ROLES, ok: true },
      { source: SRC_LOKI, ok: true },
    ];

    const input: ReadingsFactInput = { now: baseNow, probes };
    const res = deriveReadings(input);

    const board = res.readings.find((r) => r.source === SRC_BOARD);
    expect(board?.status).toBe(STATUS_FAILED);
    expect(board?.reason).toBe(REASON_CONN_REFUSED);

    const rooms = res.readings.find((r) => r.source === SRC_ROOMS);
    expect(rooms?.status).toBe(STATUS_OK);

    expect(res.unmeasured.some((u) => u.source === SRC_BOARD && u.reason === REASON_CONN_REFUSED)).toBe(true);
    expect(res.unmeasured.some((u) => u.source === SRC_DECISIONS && u.reason === REASON_NO_SOURCE)).toBe(true);
    expect(res.unmeasured.length).toBe(2);
  });
});

describe('Source Readings — Service & Endpoint', () => {
  test('ReadingsService caches derived readings and serves on timer', async () => {
    const mockFetcher = jest.fn().mockResolvedValue({
      now: baseNow,
      probes: [
        { source: SRC_BOARD, ok: true, timestamp: BOARD_TIME },
        { source: SRC_ROOMS, ok: true },
        { source: SRC_DECISIONS, ok: false, error: REASON_NO_SOURCE },
        { source: SRC_ROLES, ok: true },
        { source: SRC_LOKI, ok: true },
      ],
    });

    const service = new ReadingsService(mockFetcher, 15000);
    expect(service.getReadings().readings).toEqual([]);

    await service.refresh();
    const cached = service.getReadings();
    expect(cached.readings.length).toBeGreaterThan(0);
    expect(cached.unmeasured.length).toBe(1);

    service.stop();
  });

  test('GET /api/readings returns cached envelope with readings and unmeasured', async () => {
    const store = new MessageStore(':memory:');
    const mockService = {
      getReadings: () => ({
        derivedAt: baseNow.toISOString(),
        readings: [
          {
            source: SRC_BOARD,
            observedAt: baseNow.toISOString(),
            status: 'ok' as const,
            lastWrite: BOARD_TIME,
            asOf: 'as of 11:18',
          },
        ],
        unmeasured: [],
      }),
    };

    const app = createApp(
      store,
      undefined,
      undefined,
      undefined,
      undefined,
      mockService as unknown as ReadingsService
    );
    const res = await request(app).get('/api/readings');

    expect(res.status).toBe(200);
    expect(typeof res.body.derivedAt).toBe('string');
    expect(Array.isArray(res.body.readings)).toBe(true);
    expect(res.body.readings.length).toBe(1);
    expect(res.body.readings[0].source).toBe(SRC_BOARD);
    expect(Array.isArray(res.body.unmeasured)).toBe(true);
  });
});

describe('Source Readings — Live Fetcher & Service Resilience', () => {
  test('buildLiveReadingsFetcher probes live HTTP doors and reports ok/failed honestly', async () => {
    let failMode = false;
    const srv = http.createServer((req, res) => {
      if (failMode) {
        res.writeHead(502);
        res.end('Bad Gateway');
        return;
      }
      if (req.url === '/board/wip') {
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ data: { timestamp: BOARD_TIME } }));
      } else if (req.url === '/roles' || req.url === '/ready') {
        res.writeHead(200);
        res.end('ok');
      } else {
        res.writeHead(404);
        res.end();
      }
    });
    await new Promise<void>((r) => srv.listen(0, '127.0.0.1', () => r()));
    const port = (srv.address() as { port: number }).port;
    const base = `http://127.0.0.1:${port}`;

    try {
      const okFetcher = buildLiveReadingsFetcher({
        boardUrl: `${base}/board`,
        rolesUrl: `${base}/roles`,
        lokiUrl: base,
        buzzRelayUrl: 'ws://127.0.0.1:13000',
        timeoutMs: 1000,
      });
      const okFacts = await okFetcher();
      const okDerived = deriveReadings({ ...okFacts, probes: [...okFacts.probes, { source: 'custom', ok: true }] });
      expect(okDerived.readings.find((r) => r.source === SRC_BOARD)?.status).toBe(STATUS_OK);
      expect(okDerived.readings.find((r) => r.source === SRC_ROLES)?.status).toBe(STATUS_OK);
      expect(okDerived.readings.find((r) => r.source === SRC_LOKI)?.status).toBe(STATUS_OK);
      expect(okDerived.readings.find((r) => r.source === SRC_ROOMS)?.status).toBe(STATUS_OK);
      expect(okDerived.readings.find((r) => r.source === 'custom')?.status).toBe(STATUS_OK);

      failMode = true;
      const failFetcher = buildLiveReadingsFetcher({
        boardUrl: `${base}/board`,
        rolesUrl: `${base}/roles`,
        lokiUrl: base,
        buzzRelayUrl: '',
        timeoutMs: 1000,
      });
      const failFacts = await failFetcher();
      const failDerived = deriveReadings(failFacts);
      expect(failDerived.readings.find((r) => r.source === SRC_BOARD)?.status).toBe(STATUS_FAILED);
      expect(failDerived.readings.find((r) => r.source === SRC_ROLES)?.status).toBe(STATUS_FAILED);
      expect(failDerived.readings.find((r) => r.source === SRC_LOKI)?.status).toBe(STATUS_FAILED);
      expect(failDerived.readings.find((r) => r.source === SRC_ROOMS)?.reason).toBe('relay not configured');

      const connRefusedFetcher = buildLiveReadingsFetcher({
        boardUrl: 'http://127.0.0.1:1/board',
        rolesUrl: 'http://127.0.0.1:1/roles',
        lokiUrl: 'http://127.0.0.1:1',
        timeoutMs: 200,
      });
      const refusedFacts = await connRefusedFetcher();
      expect(refusedFacts.probes.find((p) => p.source === SRC_BOARD)?.ok).toBe(false);
    } finally {
      await new Promise<void>((r) => srv.close(() => r()));
    }
  });

  test('ReadingsService handles fetcher rejection and start/stop lifecycle', async () => {
    const failingFetcher = jest.fn().mockRejectedValue(new Error('probe crash'));
    const service = new ReadingsService(failingFetcher, 60000);
    service.start();
    service.start(); // idempotent
    const res = await service.refresh();
    expect(res.readings).toEqual([]);
    expect(res.unmeasured).toEqual([{ source: 'pulse:readings', reason: 'probe crash' }]);
    service.stop();
  });
});
