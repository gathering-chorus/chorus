// @test-type: integration — a local HTTP stub stands in for Loki and the door
/**
 * #4623 — the overnight fetcher asks Loki only for what's new.
 *
 * Measured 2026-10-18: pulse re-read every card move from Jeff's last browser
 * session end on every 15s tick, and Loki split each of those reads into about
 * 150 internal queries. These tests count the range each tick asks Loki for.
 */
import http from 'http';
import { buildLiveChangesOvernightFetcher, deriveChangesOvernight, LOKI_REREAD_OVERLAP_MS } from './changes-overnight';

const P_JEFF = 'principal-jeff';
const CH_BROWSER = 'browser';
const HOUR_MS = 3600_000;

interface Fake {
  base: string;
  starts: number[];
  ends: number[];
  watermarkIso: string;
  moves: Array<[string, string]>;
  close: () => Promise<void>;
}

function moveLine(card: number, at: Date): [string, string] {
  const line = JSON.stringify({ event: 'card.accepted', card_id: `#${card}`, title: `card ${card}`, role: 'kade', timestamp: at.toISOString() });
  return [`${at.getTime()}000000`, line];
}

async function fakeDoorAndLoki(watermarkIso: string): Promise<Fake> {
  const fake = { starts: [] as number[], ends: [] as number[], watermarkIso, moves: [] as Array<[string, string]> } as Fake;
  const srv = http.createServer((req, res) => {
    const url = new URL(req.url || '/', 'http://x');
    res.writeHead(200, { 'Content-Type': 'application/json' });
    if (url.pathname.startsWith('/v1/identity/sessions')) {
      res.end(JSON.stringify({ data: [{ name: 's', ownedBy: P_JEFF, channel: CH_BROWSER, sessionState: 'closed', endedAt: fake.watermarkIso }] }));
      return;
    }
    const startMs = Number(BigInt(url.searchParams.get('start') || '0') / 1_000_000n);
    fake.starts.push(startMs);
    fake.ends.push(Number(BigInt(url.searchParams.get('end') || '0') / 1_000_000n));
    const values = fake.moves.filter(([ns]) => Number(BigInt(ns) / 1_000_000n) >= startMs);
    res.end(JSON.stringify({ data: { result: [{ values }] } }));
  });
  await new Promise<void>((r) => srv.listen(0, '127.0.0.1', () => r()));
  fake.base = `http://127.0.0.1:${(srv.address() as { port: number }).port}`;
  fake.close = () => new Promise<void>((r) => srv.close(() => r()));
  return fake;
}

describe('#4623 overnight fetcher reads only what is new', () => {
  test('the second tick asks Loki for minutes, not the hours since the watermark', async () => {
    const watermark = new Date(Date.now() - 16 * HOUR_MS);
    const fake = await fakeDoorAndLoki(watermark.toISOString());
    fake.moves.push(moveLine(4601, new Date(Date.now() - 10 * HOUR_MS)));
    try {
      const fetcher = buildLiveChangesOvernightFetcher(fake.base, fake.base);
      await fetcher();
      fake.moves.push(moveLine(4602, new Date()));
      const second = await fetcher();

      expect(fake.starts[0]).toBe(watermark.getTime());
      // Tick 2 starts where tick 1's read ended, less a small overlap for
      // lines promtail ships late.
      expect(fake.ends[0]).toBeGreaterThan(watermark.getTime());
      expect(fake.starts[1]).toBe(fake.ends[0] - LOKI_REREAD_OVERLAP_MS);
      expect(LOKI_REREAD_OVERLAP_MS).toBeLessThanOrEqual(5 * 60_000);
      // The early move is still reported though tick 2 never re-read it.
      const ids = deriveChangesOvernight(second).cards.map((c) => c.id).sort();
      expect(ids).toEqual([4601, 4602]);
    } finally {
      await fake.close();
    }
  });

  test('a move read twice in the overlap is counted once', async () => {
    const fake = await fakeDoorAndLoki(new Date(Date.now() - 2 * HOUR_MS).toISOString());
    fake.moves.push(moveLine(4603, new Date()));
    try {
      const fetcher = buildLiveChangesOvernightFetcher(fake.base, fake.base);
      await fetcher();
      const second = await fetcher();
      expect(second.lokiMoves?.length).toBe(1);
    } finally {
      await fake.close();
    }
  });

  test('a new watermark starts a full read from it', async () => {
    const fake = await fakeDoorAndLoki(new Date(Date.now() - 16 * HOUR_MS).toISOString());
    fake.moves.push(moveLine(4604, new Date(Date.now() - 12 * HOUR_MS)));
    try {
      const fetcher = buildLiveChangesOvernightFetcher(fake.base, fake.base);
      await fetcher();
      const newer = new Date(Date.now() - 1 * HOUR_MS);
      fake.watermarkIso = newer.toISOString();
      const after = await fetcher();
      expect(fake.starts[1]).toBe(newer.getTime());
      expect(deriveChangesOvernight(after).cards).toEqual([]);
    } finally {
      await fake.close();
    }
  });

  test('LOKI_URL=off asks Loki nothing and reports the moves unmeasured', async () => {
    const fake = await fakeDoorAndLoki(new Date(Date.now() - 2 * HOUR_MS).toISOString());
    try {
      const fetcher = buildLiveChangesOvernightFetcher(fake.base, 'off');
      const input = await fetcher();
      expect(fake.starts).toEqual([]);
      expect(input.unmeasured?.find((u) => u.source === 'loki:moves')?.reason).toBe('loki off in this environment');
    } finally {
      await fake.close();
    }
  });
});
