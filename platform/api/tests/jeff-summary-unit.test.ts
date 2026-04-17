/**
 * jeff-summary — unit tests (#2167).
 *
 * Target: 80%+ on src/jeff-summary.ts. Uses tempdir for POSTURE_BASE and
 * mocks global fetch for Loki calls.
 */

import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';

const TMP = fs.mkdtempSync(path.join(os.tmpdir(), 'jeff-summary-test-'));
process.env.POSTURE_BASE = TMP;

function load() {
  return require('../src/jeff-summary');
}

const realFetch = global.fetch;

afterAll(() => {
  try { fs.rmSync(TMP, { recursive: true, force: true }); } catch {}
});

afterEach(() => {
  (global as any).fetch = realFetch;
});

function writeScores(date: string, scores: any[]) {
  const dir = path.join(TMP, date);
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(
    path.join(dir, 'scores.jsonl'),
    scores.map((s) => JSON.stringify(s)).join('\n') + '\n',
  );
}

function clear() {
  for (const f of fs.readdirSync(TMP)) fs.rmSync(path.join(TMP, f), { recursive: true, force: true });
}

describe('getPostureStrip', () => {
  beforeEach(() => { clear(); jest.resetModules(); });

  test('returns empty when POSTURE_BASE does not exist', () => {
    process.env.POSTURE_BASE = path.join(TMP, 'nonexistent');
    jest.resetModules();
    const { getPostureStrip } = load();
    expect(getPostureStrip(7, 'all', 'all')).toEqual({ frames: [], total: 0, filtered: 0, days: 7 });
    process.env.POSTURE_BASE = TMP;
  });

  test('clamps days to [1, 30]', () => {
    const { getPostureStrip } = load();
    expect(getPostureStrip(0, 'all', 'all').days).toBe(1);
    expect(getPostureStrip(1000, 'all', 'all').days).toBe(30);
    expect(getPostureStrip(15, 'all', 'all').days).toBe(15);
  });

  test('reads scores.jsonl entries across dates (newest first)', () => {
    writeScores('2026-04-15', [
      { posture: 'upright', mood: 'calm', tension: 'low', breath: 'slow', energy: 'med', expression: 'open', notes: '', timestamp: '2026-04-15T10:00:00Z', image: 'a.jpg' },
    ]);
    writeScores('2026-04-17', [
      { posture: 'slouched', mood: 'focused', tension: 'mid', breath: 'steady', energy: 'high', expression: 'focused', notes: '', timestamp: '2026-04-17T10:00:00Z', image: 'b.jpg' },
    ]);
    const { getPostureStrip } = load();
    const r = getPostureStrip(30, 'all', 'all');
    expect(r.total).toBe(2);
    // Dirs read reverse-sorted (newest first) — 2026-04-17 comes before 2026-04-15
    expect(r.frames[0].date).toBe('2026-04-17');
    expect(r.frames[1].date).toBe('2026-04-15');
  });

  test('dirs without scores.jsonl are skipped', () => {
    fs.mkdirSync(path.join(TMP, '2026-04-16'), { recursive: true });
    writeScores('2026-04-17', [
      { posture: 'upright', mood: 'calm', tension: 'low', breath: 'slow', energy: 'med', expression: 'open', notes: '', timestamp: '2026-04-17T10:00:00Z', image: 'a.jpg' },
    ]);
    const { getPostureStrip } = load();
    expect(getPostureStrip(30, 'all', 'all').total).toBe(1);
  });

  test('malformed scores file is skipped entirely (try/catch wraps whole file)', () => {
    // The current implementation wraps the for-loop in a single try/catch, so
    // any malformed line aborts the file. Test codifies the actual behavior.
    const dir = path.join(TMP, '2026-04-17');
    fs.mkdirSync(dir, { recursive: true });
    fs.writeFileSync(path.join(dir, 'scores.jsonl'), 'not json\n{"posture":"upright","mood":"ok","tension":"low","breath":"s","energy":"m","expression":"e","notes":"","timestamp":"t","image":"i"}\n');
    // Second file is fully valid — proves the overall fn still works on good files.
    writeScores('2026-04-16', [
      { posture: 'upright', mood: 'ok', tension: 'low', breath: 's', energy: 'm', expression: 'e', notes: '', timestamp: 't', image: 'i' },
    ]);
    const { getPostureStrip } = load();
    expect(getPostureStrip(30, 'all', 'all').total).toBe(1);  // only the good file
  });

  test('posture filter narrows frames', () => {
    writeScores('2026-04-17', [
      { posture: 'upright', mood: 'calm', tension: 'low', breath: 's', energy: 'm', expression: 'e', notes: '', timestamp: 't1', image: 'a' },
      { posture: 'slouched', mood: 'calm', tension: 'low', breath: 's', energy: 'm', expression: 'e', notes: '', timestamp: 't2', image: 'b' },
    ]);
    const { getPostureStrip } = load();
    const r = getPostureStrip(30, 'upright', 'all');
    expect(r.total).toBe(2);
    expect(r.filtered).toBe(1);
    expect(r.frames[0].posture).toBe('upright');
  });

  test('mood filter narrows frames', () => {
    writeScores('2026-04-17', [
      { posture: 'u', mood: 'focused', tension: 'l', breath: 's', energy: 'm', expression: 'e', notes: '', timestamp: 't1', image: 'a' },
      { posture: 'u', mood: 'tired', tension: 'l', breath: 's', energy: 'm', expression: 'e', notes: '', timestamp: 't2', image: 'b' },
    ]);
    const { getPostureStrip } = load();
    const r = getPostureStrip(30, 'all', 'focused');
    expect(r.filtered).toBe(1);
    expect(r.frames[0].mood).toBe('focused');
  });

  test('combined filters narrow further', () => {
    writeScores('2026-04-17', [
      { posture: 'upright', mood: 'focused', tension: 'l', breath: 's', energy: 'm', expression: 'e', notes: '', timestamp: 't1', image: 'a' },
      { posture: 'upright', mood: 'tired', tension: 'l', breath: 's', energy: 'm', expression: 'e', notes: '', timestamp: 't2', image: 'b' },
      { posture: 'slouched', mood: 'focused', tension: 'l', breath: 's', energy: 'm', expression: 'e', notes: '', timestamp: 't3', image: 'c' },
    ]);
    const { getPostureStrip } = load();
    const r = getPostureStrip(30, 'upright', 'focused');
    expect(r.filtered).toBe(1);
  });
});

describe('getWerkActivity', () => {
  beforeEach(() => { jest.resetModules(); });

  function makeLokiResp(values: Array<[string, string]>) {
    return new Response(JSON.stringify({ data: { result: [{ values }] } }), {
      status: 200,
      headers: { 'content-type': 'application/json' },
    });
  }

  test('clamps hours to [1, 168]', async () => {
    (global as any).fetch = jest.fn(() => Promise.resolve(makeLokiResp([])));
    const { getWerkActivity } = load();
    expect((await getWerkActivity(0, '', '')).hours).toBe(1);
    expect((await getWerkActivity(10_000, '', '')).hours).toBe(168);
    expect((await getWerkActivity(24, '', '')).hours).toBe(24);
  });

  test('merges all four streams and counts by source', async () => {
    const entry = (e: any) => ['1000000000', JSON.stringify(e)] as [string, string];
    const calls: string[] = [];
    (global as any).fetch = jest.fn((url: string) => {
      calls.push(url);
      if (url.includes('board-client')) return Promise.resolve(makeLokiResp([entry({ event: 'card.pulled' })]));
      if (url.includes('chorus-session')) return Promise.resolve(makeLokiResp([entry({ event: 'session.start' })]));
      if (url.includes('interaction.pattern')) return Promise.resolve(makeLokiResp([entry({ event: 'interaction.pattern' })]));
      // chorus-events (falls through)
      return Promise.resolve(makeLokiResp([entry({ event: 'role.nudge.sent' }), entry({ event: 'role.nudge.sent' })]));
    });
    const { getWerkActivity } = load();
    const r = await getWerkActivity(1, '', '');
    expect(r.total).toBe(5);
    expect(r.sources['board-client']).toBe(1);
    expect(r.sources['chorus-events']).toBe(2);
    expect(r.sources['chorus-session']).toBe(1);
    expect(r.sources['interaction.pattern']).toBe(1);
    // 4 fetches
    expect(calls).toHaveLength(4);
  });

  test('role filter interpolates into query suffix', async () => {
    const fetchSpy = jest.fn(() => Promise.resolve(makeLokiResp([])));
    (global as any).fetch = fetchSpy;
    const { getWerkActivity } = load();
    await getWerkActivity(1, 'KADE', '');
    const urls = fetchSpy.mock.calls.map((c) => decodeURIComponent(c[0] as string));
    expect(urls.some((u) => u.includes('role="kade"'))).toBe(true);
  });

  test('event filter is applied as =~ regex', async () => {
    const fetchSpy = jest.fn(() => Promise.resolve(makeLokiResp([])));
    (global as any).fetch = fetchSpy;
    const { getWerkActivity } = load();
    await getWerkActivity(1, '', 'card\\..*');
    const urls = fetchSpy.mock.calls.map((c) => decodeURIComponent(c[0] as string));
    expect(urls.some((u) => u.includes('event=~"card\\..*"'))).toBe(true);
  });

  test('malformed log line falls through to raw field', async () => {
    (global as any).fetch = jest.fn(() => Promise.resolve(makeLokiResp([['1', 'not json']])));
    const { getWerkActivity } = load();
    const r = await getWerkActivity(1, '', '');
    expect(r.total).toBeGreaterThan(0);
    expect(r.entries[0]).toHaveProperty('raw', 'not json');
  });

  test('entries sorted descending by timestamp', async () => {
    const vals: Array<[string, string]> = [
      ['100', JSON.stringify({ a: 1 })],
      ['300', JSON.stringify({ a: 3 })],
      ['200', JSON.stringify({ a: 2 })],
    ];
    (global as any).fetch = jest.fn(() => Promise.resolve(makeLokiResp(vals)));
    const { getWerkActivity } = load();
    const r = await getWerkActivity(1, '', '');
    const timestamps = r.entries.map((e: any) => Number(e.ts));
    // Sort check across every stream — newest first
    for (let i = 1; i < timestamps.length; i++) {
      expect(timestamps[i - 1]).toBeGreaterThanOrEqual(timestamps[i]);
    }
  });

  test('Loki non-OK returns empty values (no crash)', async () => {
    (global as any).fetch = jest.fn(() => Promise.resolve(new Response('', { status: 500 })));
    const { getWerkActivity } = load();
    const r = await getWerkActivity(1, '', '');
    expect(r.total).toBe(0);
  });

  test('Loki network error returns empty values', async () => {
    (global as any).fetch = jest.fn(() => Promise.reject(new Error('ECONNREFUSED')));
    const { getWerkActivity } = load();
    const r = await getWerkActivity(1, '', '');
    expect(r.total).toBe(0);
  });

  test('Loki response without values array is tolerated', async () => {
    (global as any).fetch = jest.fn(() => Promise.resolve(
      new Response(JSON.stringify({ data: { result: [{}] } }), {
        status: 200, headers: { 'content-type': 'application/json' },
      })
    ));
    const { getWerkActivity } = load();
    const r = await getWerkActivity(1, '', '');
    expect(r.total).toBe(0);
  });
});
