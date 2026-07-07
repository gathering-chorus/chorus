// @test-type: unit — BoardClient with the api() HTTP layer fully stubbed and the
// cache dir a per-test tmp dir (CARDS_CACHE_DIR); no live Vikunja, no $HOME writes.
/**
 * #3625 AC3 — cards stops re-sweeping the whole board on every invocation.
 *
 * Jeff's experience under test: during the 2026-07-07 OOM, concurrent agent
 * turns drove 26 full 73-page board sweeps per minute into Vikunja (9,800
 * requests/5min) — pure waste amplifying a memory spiral. With the cache, a
 * repeat read within the TTL costs zero HTTP; a mutation invalidates so the
 * next read is fresh; the cache can never serve a wrong answer to the
 * resolveIndex fresh-scan fallback.
 *
 * The test brings its own world: cache dir is a per-test tmp dir via
 * CARDS_CACHE_DIR; no live service, no $HOME writes.
 */
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { fileTaskCache } from '../src/task-cache';
import { BoardClient } from '../src/client';
import { VikunjaTask, BoardConfig } from '../src/types';

let dir: string;

beforeEach(() => {
  dir = fs.mkdtempSync(path.join(os.tmpdir(), 'cards-cache-'));
  process.env.CARDS_CACHE_DIR = dir;
  delete process.env.CARDS_CACHE_DISABLE;
  delete process.env.CARDS_CACHE_TTL_MS;
});

afterEach(() => {
  fs.rmSync(dir, { recursive: true, force: true });
  delete process.env.CARDS_CACHE_DIR;
});

const task = (id: number): VikunjaTask =>
  ({ id, index: id, title: `t${id}`, done: false, description: '', labels: [] } as unknown as VikunjaTask);

const board = { name: 'gathering', projectId: 2, viewId: 1 } as unknown as BoardConfig;

/** Client whose HTTP layer is a scripted page sequence — no live Vikunja. */
function stubApi(client: BoardClient, pages: VikunjaTask[][]) {
  let call = 0;
  const api = jest.fn(async () => pages[call++] ?? []);
  (client as unknown as { api: unknown }).api = api;
  return api;
}

describe('fileTaskCache', () => {
  test('write then read within TTL returns the tasks', () => {
    const cache = fileTaskCache(2);
    cache.write([task(1), task(2)]);
    const got = cache.read();
    expect(got).not.toBeNull();
    expect(got!.map(t => t.id)).toEqual([1, 2]);
  });

  test('expired TTL reads as a miss', () => {
    process.env.CARDS_CACHE_TTL_MS = '1';
    const cache = fileTaskCache(2);
    cache.write([task(1)]);
    const until = Date.now() + 10;
    while (Date.now() < until) { /* let the 1ms TTL lapse */ }
    expect(cache.read()).toBeNull();
  });

  test('invalidate removes the cache', () => {
    const cache = fileTaskCache(2);
    cache.write([task(1)]);
    cache.invalidate();
    expect(cache.read()).toBeNull();
  });

  test('corrupt cache file reads as a miss, never throws', () => {
    const cache = fileTaskCache(2);
    cache.write([task(1)]);
    const file = fs.readdirSync(dir).map(f => path.join(dir, f))[0];
    fs.writeFileSync(file, '{not json');
    expect(cache.read()).toBeNull();
  });

  test('CARDS_CACHE_DISABLE turns the cache off entirely', () => {
    process.env.CARDS_CACHE_DISABLE = '1';
    const cache = fileTaskCache(2);
    cache.write([task(1)]);
    expect(cache.read()).toBeNull();
  });

  test('projects do not share cache entries', () => {
    const a = fileTaskCache(2);
    const b = fileTaskCache(9);
    a.write([task(1)]);
    expect(b.read()).toBeNull();
  });
});

describe('BoardClient.fetchAllTasks through the cache', () => {
  test('second fetch within TTL costs zero HTTP calls', async () => {
    const client = new BoardClient('http://localhost:3456', 'tok', board);
    const api = stubApi(client, [[task(1), task(2)], []]);
    await client.fetchAllTasks();
    const pagesFetched = api.mock.calls.length; // page1 + empty terminator
    await client.fetchAllTasks();
    expect(api.mock.calls.length).toBe(pagesFetched);
  });

  test('clearCache (the mutation path) makes the next fetch fresh', async () => {
    const client = new BoardClient('http://localhost:3456', 'tok', board);
    const api = stubApi(client, [[task(1)], [], [task(1), task(99)], []]);
    await client.fetchAllTasks();
    client.clearCache();
    const got = await client.fetchAllTasks();
    expect(api.mock.calls.length).toBe(4);
    expect(got.map(t => t.id)).toContain(99);
  });

  test('fresh=true bypasses the cache read (resolveIndex fallback contract)', async () => {
    const client = new BoardClient('http://localhost:3456', 'tok', board);
    const api = stubApi(client, [[task(1)], [], [task(1), task(50)], []]);
    await client.fetchAllTasks();
    const got = await client.fetchAllTasks(true);
    expect(api.mock.calls.length).toBe(4);
    expect(got.map(t => t.id)).toContain(50);
  });

  test('a second client (separate process stand-in) reuses the disk cache', async () => {
    const first = new BoardClient('http://localhost:3456', 'tok', board);
    stubApi(first, [[task(1)], []]);
    await first.fetchAllTasks();

    const second = new BoardClient('http://localhost:3456', 'tok', board);
    const apiB = stubApi(second, [[task(7)], []]);
    const got = await second.fetchAllTasks();
    expect(apiB.mock.calls.length).toBe(0);
    expect(got.map(t => t.id)).toEqual([1]);
  });
});
