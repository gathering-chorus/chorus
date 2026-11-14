// @domain: awareness
// @test-type: unit — DeliveryWorker in delivery-worker.ts against a temp sqlite store and injected inject/spine doubles; no live services
// @card: #4965
// @owner: kade
/**
 * #4965 — the delivery worker's defaults and deferred families: a busy target
 * parks the row queued, an undelivered-* defer fails it with that reason, a
 * transient failure retries on the default sleep, the default smoke test
 * passes, and a cooldown read that throws turns dedupe off on the spine.
 */
import fs from 'fs';
import os from 'os';
import path from 'path';
import { DeliveryWorker, type InjectResult, type RunInject } from './delivery-worker';
import { MessageStore } from './store';

let dir: string;
let store: MessageStore;
let events: Array<{ event: string; fields: Record<string, unknown> }>;
const emit = (event: string, fields: Record<string, unknown>) => { events.push({ event, fields }); return Promise.resolve(); };

beforeEach(() => {
  dir = fs.mkdtempSync(path.join(os.tmpdir(), 'dw-cov-4965-'));
  store = new MessageStore(path.join(dir, 'm.db'));
  events = [];
});
afterEach(() => {
  store.close();
  fs.rmSync(dir, { recursive: true, force: true });
});

const rowOf = (id: number) => ({ id, from: 'silas', to: 'wren', content: 'deliver me', delivery_attempts: 0 });
const injectOnce = (r: InjectResult): RunInject => () => Promise.resolve(r);

describe('#4965 deferred deliveries settle by their reason', () => {
  test('a target-busy defer parks the row queued, not delivered', async () => {
    const id = store.sendNudge('silas', 'wren', 'deliver me');
    const w = new DeliveryWorker(store, injectOnce({ rc: 0, stderr: '', deferred: true, deferReason: 'target-busy' }), emit, [1]);
    await w.enqueue(rowOf(id));
    const rec = store.getDeliveryRecord(id);
    expect(rec.delivery_status).toBe('queued');
    expect(rec.last_delivery_error).toBe('target-busy');
    expect(events.find((e) => e.event === 'nudge.deferred')?.fields).toMatchObject({ id, reason: 'target-busy' });
  });

  test('negative: an undelivered-* defer fails the row with that reason, never delivered', async () => {
    const id = store.sendNudge('silas', 'wren', 'deliver me');
    const w = new DeliveryWorker(store, injectOnce({ rc: 0, stderr: '', deferred: true, deferReason: 'undelivered-unregistered' }), emit, [1]);
    await w.enqueue(rowOf(id));
    const rec = store.getDeliveryRecord(id);
    expect(rec.delivery_status).toBe('failed');
    expect(rec.last_delivery_error).toBe('undelivered-unregistered');
    expect(events.some((e) => e.event === 'nudge.surfaced')).toBe(false);
  });
});

describe('#4965 the worker defaults', () => {
  test('a transient failure retries after the default sleep and then delivers', async () => {
    const id = store.sendNudge('silas', 'wren', 'deliver me');
    const results: InjectResult[] = [{ rc: 1, stderr: 'osascript hiccup' }, { rc: 0, stderr: '', target: 'tmux:%3' }];
    const calls: number[] = [];
    const w = new DeliveryWorker(store, () => { calls.push(1); return Promise.resolve(results.shift()!); }, emit, [5]);
    await w.enqueue(rowOf(id));
    expect(calls).toHaveLength(2);
    expect(store.getDeliveryRecord(id).delivery_status).toBe('delivered');
    const failed = events.find((e) => e.event === 'nudge.surface.failed');
    expect(failed?.fields).toMatchObject({ attempt: 1, permanent: false, reason: 'osascript hiccup' });
    expect(events.find((e) => e.event === 'nudge.surfaced')?.fields).toMatchObject({ attempt: 2, target: 'tmux:%3' });
  });

  test('the default self-test passes the startup smoke and says so on the spine', async () => {
    const w = new DeliveryWorker(store, injectOnce({ rc: 0, stderr: '' }), emit);
    await expect(w.startupSmoke()).resolves.toBeUndefined();
    expect(events.map((e) => e.event)).toEqual(['nudge.health.smoke_ok']);
  });

  test('negative: a cooldown read that throws reads as unread with dedupe off', async () => {
    const w = new DeliveryWorker(store, injectOnce({ rc: 0, stderr: '' }), emit);
    const stop = w.watchCooldown(60_000, () => Promise.reject(new Error('door gone')));
    await new Promise((r) => setImmediate(r));
    stop();
    expect(events).toEqual([{ event: 'announce.cooldown.unread', fields: { key: expect.any(String), error: 'Error: door gone', dedupe: 'off' } }]);
    expect(w.cooldownMs()).toBeUndefined();
  });

  test('the cooldown is re-read on every period and a good value is used', async () => {
    const w = new DeliveryWorker(store, injectOnce({ rc: 0, stderr: '' }), emit);
    let reads = 0;
    const stop = w.watchCooldown(10, () => { reads += 1; return Promise.resolve({ ok: true as const, value: '1500' }); });
    await new Promise((r) => setTimeout(r, 60));
    stop();
    expect(reads).toBeGreaterThanOrEqual(2);
    expect(w.cooldownMs()).toBe(1500);
    // the same value read again is said once, not on every period
    expect(events.filter((e) => e.event === 'announce.cooldown.read')).toHaveLength(1);
  });
});
