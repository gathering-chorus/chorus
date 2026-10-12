// @test-type: integration — drives real requests through the express app
/**
 * #4508 — a chorus-api stall alert names what was running. 117 stall alerts on
 * 2026-10-12, 86 naming no cause: the outside probe announced while the loop was
 * still stuck. Now both detectors read a request ledger (every request,
 * served files included) once the loop answers, and the alert says which
 * requests overlapped the stall, or that none did.
 */
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
const TMP = fs.mkdtempSync(path.join(os.tmpdir(), 'chorus-api-4508-'));
process.env.CHORUS_ROOT = TMP;
process.env.DB_PATH = path.join(TMP, 'test.db');
process.env.CHORUS_LOG_PATH = path.join(TMP, 'chorus.log');
process.env.CLAUDE_STATS_CACHE = path.join(TMP, 'stats.json');
process.env.CLEARING_TRANSCRIPTS_DIR = path.join(TMP, 'transcripts');
process.env.QUALITY_CACHE_PATH = path.join(TMP, 'quality.json');
process.env.POSTURE_BASE = path.join(TMP, 'posture');
process.env.SESSIONS_DIR = path.join(TMP, 'sessions');

import app from '../src/server';
import type { AddressInfo } from 'net';
import type { Server } from 'http';
import { RequestLedger, describeOverlap } from '../src/request-ledger';
import { runEventloopProbe } from '../src/eventloop-probe';
import { formatBlockAlert, BlockAlert } from '../src/eventloop-alert';

const ACCESS_LINE = / GET \/index\.html 200 \d+ms \d+b$/;
const close = (srv: Server) => new Promise<void>((r) => srv.close(() => r()));
const ready = (srv: Server) => new Promise<void>((r) => (srv.listening ? r() : srv.once('listening', () => r())));

describe('#4508 request ledger', () => {
  test('names the requests that overlapped a stall, longest first, with size', () => {
    const l = new RequestLedger();
    const a = l.start('GET', '/borg/nightly-live.html', 1000);
    const b = l.start('GET', '/api/chorus/search', 2000);
    l.start('GET', '/api/chorus/context/health', 9500); // starts after the stall: not named
    l.finish(a, 200, 5_400_000, 9000);
    l.finish(b, 200, 120, 2500);
    const text = describeOverlap(l.overlapping(1500, 9000), 9600);
    expect(text).toBe('In flight during the stall: GET /borg/nightly-live.html (8.0s, 5400000 bytes); GET /api/chorus/search (0.5s, 120 bytes).');
  });

  test('an unfinished request is named as still running', () => {
    const l = new RequestLedger();
    l.start('POST', '/api/chorus/reindex', 1000);
    expect(describeOverlap(l.overlapping(2000, 3000), 4000)).toBe('In flight during the stall: POST /api/chorus/reindex (3.0s, still running).');
  });

  test('negative proof: a stall with nothing in flight says so, never a blank', () => {
    const l = new RequestLedger();
    const a = l.start('GET', '/x', 0);
    l.finish(a, 200, 1, 100);
    expect(describeOverlap(l.overlapping(500, 900), 1000)).toMatch(/^No request was in flight during the stall/);
  });
});

describe('#4508 the alert carries what was in flight', () => {
  test('in-process detector text includes the ledger sentence', () => {
    const a = formatBlockAlert(4200, '2026-10-12T21:33:40.000Z', 'unknown', undefined, 'In flight during the stall: GET /x (4.2s).');
    expect(a.message).toContain('In flight during the stall: GET /x (4.2s).');
  });

  test('the probe waits for recovery, then nudges with the measured length and the requests', async () => {
    let clock = 0;
    const lat = [8000, 8000, 50];
    let i = 0;
    const nudges: BlockAlert[] = [];
    const asked: Array<[number, number]> = [];
    await runEventloopProbe({
      probe: async () => {
        const l = lat[i++];
        clock += l;
        return { latencyMs: l, timedOut: l >= 8000 };
      },
      emit: () => {},
      nudge: (a) => nudges.push(a),
      inFlight: async (from, to) => {
        asked.push([from, to]);
        return 'In flight during the stall: GET /borg/nightly-live.html (15.9s).';
      },
      threshold: 3000, intervalMs: 0, throttleMs: 0, ticks: 3,
      now: () => clock, sleep: async () => {},
    });
    expect(nudges).toHaveLength(1);
    expect(asked).toEqual([[0, 16000]]);
    expect(nudges[0].message).toContain('blocked 16000ms'); // measured, not "≥8s ceiling"
    expect(nudges[0].message).toContain('GET /borg/nightly-live.html');
  });

  test('negative proof: while the loop is still stuck, the probe sends nothing', async () => {
    let clock = 0;
    const nudges: BlockAlert[] = [];
    await runEventloopProbe({
      probe: async () => {
        clock += 8000;
        return { latencyMs: 8000, timedOut: true };
      },
      emit: () => {},
      nudge: (a) => nudges.push(a),
      inFlight: async () => 'x',
      threshold: 3000, intervalMs: 0, throttleMs: 0, ticks: 3, now: () => clock, sleep: async () => {},
    });
    expect(nudges).toHaveLength(0);
  });
});

describe('#4508 served files reach the ledger and the access log', () => {
  test('a static file serve shows up in /eventloop/in-flight with its size', async () => {
    const srv = app.listen(0);
    await ready(srv);
    const port = (srv.address() as AddressInfo).port;
    const from = Date.now();
    const logs: string[] = [];
    const orig = console.log;
    console.log = (...a: unknown[]) => { logs.push(a.join(' ')); };
    try {
      const page = await fetch(`http://127.0.0.1:${port}/index.html`);
      await page.arrayBuffer();
      expect(page.status).toBe(200);
      const r = await fetch(`http://127.0.0.1:${port}/api/chorus/context/eventloop/in-flight?from=${from}&to=${Date.now()}`);
      const body = (await r.json()) as { text: string; entries: Array<{ path: string; bytes: number | null }> };
      const served = body.entries.find((e) => e.path === '/index.html');
      expect(served).toBeTruthy();
      expect(served!.bytes).toBeGreaterThan(0);
      expect(logs.filter((l) => ACCESS_LINE.exec(l) !== null)).toHaveLength(1);
    } finally {
      console.log = orig;
      await close(srv);
    }
  });

  test('a bad window is refused, not answered with everything', async () => {
    const srv = app.listen(0);
    await ready(srv);
    const port = (srv.address() as AddressInfo).port;
    try {
      const r = await fetch(`http://127.0.0.1:${port}/api/chorus/context/eventloop/in-flight?from=10&to=5`);
      expect(r.status).toBe(400);
    } finally {
      await close(srv);
    }
  });
});
