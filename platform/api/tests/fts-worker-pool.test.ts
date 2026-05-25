/**
 * #3086 — pool dispatcher tests with a FAKE worker (no real thread, no flakiness).
 * Covers the error-surfacing AC: every request settles — reply, error reply,
 * worker crash, or timeout — never a silent hang.
 */
import { createFtsPool, type WorkerLike } from '../src/fts-worker-pool';
import type { FtsReply } from '../src/fts-worker-core';

class FakeWorker implements WorkerLike {
  handlers: Record<string, (arg: unknown) => void> = {};
  sent: Array<{ id: number }> = [];
  postMessage(msg: unknown): void { this.sent.push(msg as { id: number }); }
  on(event: string, cb: (arg: unknown) => void): void { this.handlers[event] = cb; }
  emit(event: string, arg?: unknown): void { this.handlers[event]?.(arg); }
  reply(r: FtsReply): void { this.emit('message', r); }
  terminate(): void { /* no-op */ }
}

describe('createFtsPool (#3086)', () => {
  test('resolves a request with the rows from its matching reply', async () => {
    const fake = new FakeWorker();
    const pool = createFtsPool({ spawn: () => fake });
    const p = pool.runFtsAsync({ q: 'x', fetchLimit: 10, mode: 'fts' });
    expect(fake.sent[0].id).toBe(1);
    fake.reply({ id: fake.sent[0].id, rows: [{ id: 42 }] });
    await expect(p).resolves.toEqual([{ id: 42 }]);
    pool.shutdown();
  });

  test('correlates concurrent requests to their own replies', async () => {
    const fake = new FakeWorker();
    const pool = createFtsPool({ spawn: () => fake });
    const a = pool.runFtsAsync({ q: 'a', fetchLimit: 10, mode: 'fts' });
    const b = pool.runFtsAsync({ q: 'b', fetchLimit: 10, mode: 'fts' });
    expect(fake.sent.length).toBe(2);
    const [ia, ib] = fake.sent.map((s) => s.id);
    fake.reply({ id: ib, rows: [{ id: 2 }] }); // reply out of order
    fake.reply({ id: ia, rows: [{ id: 1 }] });
    await expect(a).resolves.toEqual([{ id: 1 }]);
    await expect(b).resolves.toEqual([{ id: 2 }]);
    pool.shutdown();
  });

  test('rejects when the worker replies with an error (surfaces, no hang)', async () => {
    const fake = new FakeWorker();
    const pool = createFtsPool({ spawn: () => fake });
    const p = pool.runFtsAsync({ q: 'x', fetchLimit: 10, mode: 'fts' });
    expect(fake.sent.length).toBe(1);
    fake.reply({ id: fake.sent[0].id, error: 'boom' });
    await expect(p).rejects.toThrow('boom');
    pool.shutdown();
  });

  test('rejects in-flight requests when the worker crashes', async () => {
    const fake = new FakeWorker();
    const pool = createFtsPool({ spawn: () => fake });
    const p = pool.runFtsAsync({ q: 'x', fetchLimit: 10, mode: 'fts' });
    expect(fake.sent.length).toBe(1);
    fake.emit('error', new Error('worker died'));
    await expect(p).rejects.toThrow('worker died');
    pool.shutdown();
  });

  test('rejects on timeout instead of hanging', async () => {
    jest.useFakeTimers();
    const fake = new FakeWorker();
    const pool = createFtsPool({ spawn: () => fake, timeoutMs: 50 });
    const p = pool.runFtsAsync({ q: 'x', fetchLimit: 10, mode: 'fts' });
    expect(fake.sent.length).toBe(1);
    const assertion = expect(p).rejects.toThrow('timeout');
    jest.advanceTimersByTime(60);
    await assertion;
    jest.useRealTimers();
    pool.shutdown();
  });
});
