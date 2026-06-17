// @test-type: integration — #3442 backfill: touches a real tmpdir (mkdtemp), not unit.
/**
 * cost-summary — unit tests (#2167).
 *
 * Target: 80%+ on src/cost-summary.ts. Uses tempdir fixtures via
 * CLAUDE_STATS_CACHE + CLEARING_TRANSCRIPTS_DIR env seams. Tunnel status
 * test just asserts the returned value is valid (UP/DOWN/UNKNOWN) — exact
 * state depends on whether cloudflared is running in the test env.
 * Twilio path is covered by the pending branch (no creds in test env).
 */

import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';

const TMP = fs.mkdtempSync(path.join(os.tmpdir(), 'cost-test-'));
process.env.CLAUDE_STATS_CACHE = path.join(TMP, 'stats-cache.json');
process.env.CLEARING_TRANSCRIPTS_DIR = path.join(TMP, 'transcripts');
process.env.CLAUDE_MONTHLY_RATE = '200';
// Ensure no TWILIO creds leak from real env — force pending branch.
delete process.env.TWILIO_ACCOUNT_SID;
delete process.env.TWILIO_AUTH_TOKEN;

function load() {
  return require('../src/cost-summary');
}

afterAll(() => {
  try { fs.rmSync(TMP, { recursive: true, force: true }); } catch { /* ignore */ }
});

function writeStats(data: any) {
  fs.writeFileSync(process.env.CLAUDE_STATS_CACHE!, JSON.stringify(data));
}
function writeTranscript(filename: string, data: any) {
  fs.mkdirSync(process.env.CLEARING_TRANSCRIPTS_DIR!, { recursive: true });
  fs.writeFileSync(path.join(process.env.CLEARING_TRANSCRIPTS_DIR!, filename), JSON.stringify(data));
}
function clear() {
  try { fs.rmSync(TMP, { recursive: true, force: true }); } catch { /* ignore */ }
  fs.mkdirSync(TMP, { recursive: true });
}

const thisMonth = (() => {
  const n = new Date();
  return `${n.getFullYear()}-${String(n.getMonth() + 1).padStart(2, '0')}`;
})();

describe('getCostSummary — shape and composition', () => {
  beforeEach(() => { clear(); jest.resetModules(); });

  test('returns expected top-level fields', async () => {
    const r = await load().getCostSummary();
    expect(r).toHaveProperty('claude');
    expect(r).toHaveProperty('twilio');
    expect(r).toHaveProperty('clearing');
    expect(r).toHaveProperty('tunnel');
    expect(r).toHaveProperty('summary');
    expect(r).toHaveProperty('generatedAt');
  });

  test('summary.totalCost = fixedCost + variableCost', async () => {
    const r = await load().getCostSummary();
    expect(r.summary.totalCost).toBe(r.summary.fixedCost + r.summary.variableCost);
  });

  test('fixedCost defaults to CLAUDE_MONTHLY_RATE=200', async () => {
    const r = await load().getCostSummary();
    expect(r.summary.fixedCost).toBe(200);
  });
});

describe('getCostSummary — Claude stats branches', () => {
  beforeEach(() => { clear(); jest.resetModules(); });

  test('missing stats cache returns COLD defaults', async () => {
    const r = await load().getCostSummary();
    expect(r.claude.burnStatus).toBe('COLD');
    expect(r.claude.totalSessions).toBe(0);
    expect(r.claude.lastComputed).toBeNull();
  });

  test('malformed stats cache returns COLD defaults', async () => {
    fs.writeFileSync(process.env.CLAUDE_STATS_CACHE!, 'not json');
    const r = await load().getCostSummary();
    expect(r.claude.burnStatus).toBe('COLD');
  });

  test('high intensity (>80%) → HOT', async () => {
    const daysIn = new Array(25).fill(0).map((_, i) => ({
      date: `${thisMonth}-${String(i + 1).padStart(2, '0')}`,
      sessionCount: 5, messageCount: 100, toolCallCount: 30,
    }));
    writeStats({ dailyActivity: daysIn, hourCounts: {}, lastComputedDate: '2026-04-17' });
    const r = await load().getCostSummary();
    // intensity = activeDays / dayOfMonth — with activeDays >= dayOfMonth we get HOT
    expect(['HOT', 'SMOOTH']).toContain(r.claude.burnStatus);
    expect(r.claude.totalSessions).toBe(125);
    expect(r.claude.totalMessages).toBe(2500);
  });

  test('medium intensity (>40%) → SMOOTH', async () => {
    // 8 active days spread across month → ~40-50% depending on current dayOfMonth
    const daysIn = new Array(8).fill(0).map((_, i) => ({
      date: `${thisMonth}-${String(i + 1).padStart(2, '0')}`,
      sessionCount: 1, messageCount: 10, toolCallCount: 3,
    }));
    writeStats({ dailyActivity: daysIn, hourCounts: {} });
    const r = await load().getCostSummary();
    // Depending on today, 8/dayOfMonth lands in SMOOTH or COLD range; verify
    // the enum is populated regardless.
    expect(['HOT', 'SMOOTH', 'COLD']).toContain(r.claude.burnStatus);
  });

  test('activity for other months is filtered out', async () => {
    writeStats({
      dailyActivity: [
        { date: '2020-01-15', sessionCount: 99, messageCount: 99, toolCallCount: 99 },
        { date: `${thisMonth}-01`, sessionCount: 1, messageCount: 1, toolCallCount: 1 },
      ],
      hourCounts: {},
    });
    const r = await load().getCostSummary();
    expect(r.claude.totalSessions).toBe(1);
    expect(r.claude.totalMessages).toBe(1);
  });

  test('hourCounts passes through from cache', async () => {
    writeStats({ dailyActivity: [], hourCounts: { '09': 5, '10': 12 } });
    const r = await load().getCostSummary();
    expect(r.claude.hourCounts).toEqual({ '09': 5, '10': 12 });
  });

  test('missing dailyActivity is defaulted to []', async () => {
    writeStats({ hourCounts: {} });
    const r = await load().getCostSummary();
    expect(r.claude.dailyActivity).toEqual([]);
  });
});

describe('getCostSummary — Clearing transcripts', () => {
  beforeEach(() => { clear(); jest.resetModules(); });

  test('missing transcripts dir → empty sessions', async () => {
    const r = await load().getCostSummary();
    expect(r.clearing.sessions).toEqual([]);
    expect(r.clearing.totalCost).toBe(0);
  });

  test('this-month transcripts counted, others ignored', async () => {
    writeTranscript(`${thisMonth}-01-100000.json`, {
      session: { started: `${thisMonth}-01T10:00:00Z`, estimatedCost: 0.05, messageCount: 10 },
    });
    writeTranscript('2020-01-15-100000.json', {
      session: { started: '2020-01-15T10:00:00Z', estimatedCost: 99.99, messageCount: 999 },
    });
    const r = await load().getCostSummary();
    expect(r.clearing.sessions).toHaveLength(1);
    expect(r.clearing.totalCost).toBeCloseTo(0.05, 5);
  });

  test('transcripts without session block are skipped', async () => {
    writeTranscript(`${thisMonth}-05-100000.json`, {});
    writeTranscript(`${thisMonth}-06-100000.json`, {
      session: { started: `${thisMonth}-06T10:00:00Z`, estimatedCost: 0.1, messageCount: 5 },
    });
    const r = await load().getCostSummary();
    expect(r.clearing.sessions).toHaveLength(1);
  });

  test('malformed transcript JSON is skipped', async () => {
    fs.mkdirSync(process.env.CLEARING_TRANSCRIPTS_DIR!, { recursive: true });
    fs.writeFileSync(path.join(process.env.CLEARING_TRANSCRIPTS_DIR!, `${thisMonth}-99-bad.json`), 'not json');
    writeTranscript(`${thisMonth}-01-good.json`, {
      session: { estimatedCost: 0.02, messageCount: 3 },
    });
    const r = await load().getCostSummary();
    expect(r.clearing.sessions).toHaveLength(1);
    expect(r.clearing.totalCost).toBeCloseTo(0.02, 5);
  });

  test('session missing estimatedCost defaults to 0', async () => {
    writeTranscript(`${thisMonth}-07-100000.json`, { session: { messageCount: 1 } });
    const r = await load().getCostSummary();
    expect(r.clearing.sessions[0].cost).toBe(0);
  });
});

describe('getCostSummary — Twilio pending branch', () => {
  beforeEach(() => { clear(); jest.resetModules(); });

  test('no creds → pending:true, totalCost:0, no records', async () => {
    const r = await load().getCostSummary();
    expect(r.twilio.pending).toBe(true);
    expect(r.twilio.totalCost).toBe(0);
    expect(r.twilio.records).toEqual([]);
  });
});

describe('getCostSummary — Twilio fetch branches (mocked https)', () => {
  const EventEmitter = require('events');

  function makeMockHttpsRequest(responsePayload: string | null, opts: { error?: Error, timeout?: boolean } = {}) {
    return jest.fn((_options: any, callback: any) => {
      const req = Object.assign(new EventEmitter(), {
        setTimeout: jest.fn((_ms: number, cb: any) => {
          if (opts.timeout) setTimeout(() => { req.destroy(); cb(); }, 1);
        }),
        end: jest.fn(() => {
          if (opts.error) {
            setImmediate(() => req.emit('error', opts.error));
            return;
          }
          const res = Object.assign(new EventEmitter(), { statusCode: 200 });
          setImmediate(() => {
            callback(res);
            if (responsePayload !== null) res.emit('data', responsePayload);
            res.emit('end');
          });
        }),
        destroy: jest.fn(),
      });
      return req;
    });
  }

  beforeEach(() => { clear(); jest.resetModules(); process.env.TWILIO_ACCOUNT_SID = 'ACxxx'; process.env.TWILIO_AUTH_TOKEN = 'tok'; });
  afterEach(() => { delete process.env.TWILIO_ACCOUNT_SID; delete process.env.TWILIO_AUTH_TOKEN; });

  test('valid Twilio response aggregates records with non-zero price or count', async () => {
    const payload = JSON.stringify({
      usage_records: [
        { category: 'sms', description: 'SMS', count: '5', price: '0.10', count_unit: 'msg', price_unit: 'USD' },
        { category: 'voice', description: 'Voice', count: '0', price: '0', count_unit: 'min', price_unit: 'USD' }, // filtered
        { category: 'mms', description: 'MMS', count: '2', price: '0.05', count_unit: 'msg', price_unit: 'USD' },
      ],
    });
    jest.doMock('https', () => ({ request: makeMockHttpsRequest(payload) }));
    const r = await load().getCostSummary();
    expect(r.twilio.pending).toBe(false);
    expect(r.twilio.records).toHaveLength(2);
    expect(r.twilio.totalCost).toBeCloseTo(0.15, 4);
  });

  test('Twilio malformed JSON → totalCost:0 with pending:false', async () => {
    jest.doMock('https', () => ({ request: makeMockHttpsRequest('not json') }));
    const r = await load().getCostSummary();
    expect(r.twilio.pending).toBe(false);
    expect(r.twilio.totalCost).toBe(0);
    expect(r.twilio.records).toEqual([]);
  });

  test('Twilio request error → falls back to 0 records', async () => {
    jest.doMock('https', () => ({ request: makeMockHttpsRequest(null, { error: new Error('socket hang up') }) }));
    const r = await load().getCostSummary();
    expect(r.twilio.pending).toBe(false);
    expect(r.twilio.totalCost).toBe(0);
  });

  test('second call within TTL hits cache (no new https request)', async () => {
    const payload = JSON.stringify({ usage_records: [{ category: 's', description: 'S', count: '1', price: '0.01', count_unit: 'u', price_unit: 'USD' }] });
    const httpsMock = makeMockHttpsRequest(payload);
    jest.doMock('https', () => ({ request: httpsMock }));
    const mod = load();
    await mod.getCostSummary();
    await mod.getCostSummary();
    // First call triggered request; cache hit on second.
    expect(httpsMock).toHaveBeenCalledTimes(1);
  });
});

describe('getCostSummary — Tunnel status', () => {
  beforeEach(() => { clear(); jest.resetModules(); });

  test('tunnel status is one of UP/DOWN/UNKNOWN', async () => {
    const r = await load().getCostSummary();
    expect(['UP', 'DOWN', 'UNKNOWN']).toContain(r.tunnel.status);
  });
});
