// @domain: awareness
// @test-type: unit — cachedCards in platform/pulse/src/pulse-header.ts over a fake /cws read; no live services
/**
 * #4807 — /pulse hung for every role's prompt hook. Its cards line runs the /cws script
 * (~2s) behind a 30s cache, and when the cache expired every concurrent /pulse started its
 * own copy of the script. Under four roles' prompts the copies piled up and /pulse stopped
 * answering, so about half of Jeff's prompts lost their context.
 */
import { cachedCards } from './pulse-header';
import type { CwsRow } from './card-steps';

const row = (card: string): CwsRow => ({ card, role: 'kade', step: 'no run yet', live: false, since: '', run: '' });
const tick = (ms: number) => new Promise(r => setTimeout(r, ms));

/** A /cws read that counts its calls and answers only when told to. */
function slowRead() {
  let calls = 0;
  const pending: Array<(rows: CwsRow[]) => void> = [];
  const read = () => { calls++; return new Promise<CwsRow[]>(res => pending.push(res)); };
  return { read, calls: () => calls, answer: (rows: CwsRow[]) => pending.splice(0).forEach(r => r(rows)) };
}

/** The cache as it stood before #4807: each caller past the TTL starts its own read. */
function cachedCardsBefore4807(read: () => Promise<CwsRow[]>, ttlMs: number) {
  let last: { at: number; rows: CwsRow[] } | undefined;
  return async () => {
    if (last && Date.now() - last.at < ttlMs) return last.rows;
    const rows = await read();
    last = { at: Date.now(), rows };
    return rows;
  };
}

/** How many /cws reads N concurrent callers start once the cache has expired. */
async function readsAfterExpiry(src: ReturnType<typeof slowRead>, cards: () => Promise<CwsRow[]>, n: number) {
  const first = cards(); src.answer([row('1')]); await first;
  await tick(20);
  const before = src.calls();
  const all = Array.from({ length: n }, () => cards());
  await tick(5);
  const started = src.calls() - before;
  src.answer([row('2')]);
  await Promise.all(all);
  return started;
}

test('AC1: concurrent /pulse requests during a refresh start ONE /cws read', async () => {
  const src = slowRead();
  expect(await readsAfterExpiry(src, cachedCards(src.read, 10), 8)).toBe(1);
});

test('AC1: the very first read is shared too', async () => {
  const src = slowRead();
  const cards = cachedCards(src.read, 30_000);
  const all = Array.from({ length: 5 }, () => cards());
  await tick(5);
  expect(src.calls()).toBe(1);
  src.answer([row('1')]);
  expect(await Promise.all(all)).toEqual(Array(5).fill([row('1')]));
});

test('AC2: while a refresh runs, /pulse answers with the last cards at once', async () => {
  const src = slowRead();
  const cards = cachedCards(src.read, 10);
  const first = cards(); src.answer([row('1')]); await first;
  await tick(20);
  // the refresh never answers: the caller must not wait on it
  const t0 = Date.now();
  expect(await cards()).toEqual([row('1')]);
  expect(Date.now() - t0).toBeLessThan(50);
  expect(src.calls()).toBe(2);
});

test('AC2: a refresh that fails keeps the last cards', async () => {
  let calls = 0;
  const cards = cachedCards(() => (++calls === 1 ? Promise.resolve([row('1')]) : Promise.reject(new Error('dagu down'))), 10);
  await cards();
  await tick(20);
  expect(await cards()).toEqual([row('1')]);
  await tick(5);
  expect(await cards()).toEqual([row('1')]);
});

test('a refresh that fails is cleared, so the next expiry starts a new read (no stuck promise)', async () => {
  let calls = 0;
  const cards = cachedCards(() => (++calls === 2 ? Promise.reject(new Error('dagu down')) : Promise.resolve([row(String(calls))])), 10);
  await cards();
  await tick(20);
  await cards();              // starts refresh 2, which rejects
  await tick(20);
  await cards();              // refresh 2 is gone: this starts refresh 3
  await tick(5);
  expect(calls).toBe(3);
  expect(await cards()).toEqual([row('3')]);
});

test('a /cws read that never answers times out and is cleared', async () => {
  const src = slowRead();
  const cards = cachedCards(src.read, 10, 30);
  const first = cards(); src.answer([row('1')]); await first;
  await tick(20);
  await cards();              // refresh 2 never answers
  await tick(40);             // past the 30ms read timeout
  await cards();              // refresh 2 timed out and was cleared: this starts refresh 3
  expect(src.calls()).toBe(3);
});

test('a timed-out read is aborted, so its /cws child is killed', async () => {
  const signals: AbortSignal[] = [];
  const cards = cachedCards(signal => { signals.push(signal!); return new Promise<CwsRow[]>(() => undefined); }, 10, 30, 1_000);
  void cards().catch(() => undefined);
  await tick(5);
  expect(signals[0].aborted).toBe(false);
  await tick(40);
  expect(signals[0].aborted).toBe(true);
});

test('with nothing read yet, a caller waits at most coldWaitMs, then the line is unmeasured', async () => {
  const src = slowRead();
  const cards = cachedCards(src.read, 30_000, 10_000, 30);
  const t0 = Date.now();
  await expect(cards()).rejects.toThrow('/cws refreshing');
  expect(Date.now() - t0).toBeLessThan(200);
  src.answer([row('1')]);       // the read carried on and lands for the next caller
  await tick(5);
  expect(await cards()).toEqual([row('1')]);
  expect(src.calls()).toBe(1);
});

test('rows served past the TTL say how old they are', async () => {
  let calls = 0;
  const cards = cachedCards(() => (++calls === 1 ? Promise.resolve([row('1')]) : Promise.reject(new Error('dagu down'))), 10);
  await cards();
  expect(cards.staleSeconds()).toBe(0);
  await tick(1_100);
  expect(await cards()).toEqual([row('1')]);
  expect(cards.staleSeconds()).toBeGreaterThanOrEqual(1);
});

test('an unreadable board on the first read is still an error, not an empty board', async () => {
  const cards = cachedCards(() => Promise.reject(new Error('chorus-werk-status --json: exit 1')), 30_000);
  await expect(cards()).rejects.toThrow('chorus-werk-status');
});

test('AC3 negative proof: the same count sees 8 reads from the pre-#4807 cache and 1 from cachedCards', async () => {
  const before = slowRead();
  expect(await readsAfterExpiry(before, cachedCardsBefore4807(before.read, 10), 8)).toBe(8);
  const after = slowRead();
  expect(await readsAfterExpiry(after, cachedCards(after.read, 10), 8)).toBe(1);
});
