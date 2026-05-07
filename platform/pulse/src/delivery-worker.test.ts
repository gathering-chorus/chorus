/**
 * Delivery Worker Tests (#2727 AC2)
 *
 * Tests describe the contract: enqueue → runInject → terminal state +
 * spine event. All deps injected; no real chorus-inject, no real chorus.log.
 */

import { DeliveryWorker, classifyInjectResult, PERMANENT_REASONS, DEFAULT_BACKOFF_MS, type InjectResult, type DeliveryRow } from './delivery-worker';
import { MessageStore } from './store';
import * as fs from 'fs';
import * as path from 'path';

const TEST_DB = path.join(__dirname, '..', 'test-delivery-worker.db');

let store: MessageStore;

beforeEach(() => {
  if (fs.existsSync(TEST_DB)) fs.unlinkSync(TEST_DB);
  store = new MessageStore(TEST_DB);
});

afterEach(() => {
  try { store.close(); } catch { /* already closed */ }
  if (fs.existsSync(TEST_DB)) fs.unlinkSync(TEST_DB);
});

function rowFor(id: number, to = 'wren', content = 'test'): DeliveryRow {
  return { id, from: 'silas', to, content, delivery_attempts: 0 };
}

describe('classifyInjectResult', () => {
  test('rc=0 is success', () => {
    expect(classifyInjectResult({ rc: 0, stderr: '' })).toEqual({ kind: 'success', reason: 'ok' });
  });

  test('tcc-denied stderr is permanent', () => {
    const r = classifyInjectResult({ rc: 1, stderr: 'TCC-denied: terminal not authorized' });
    expect(r.kind).toBe('permanent');
    expect(r.reason).toBe('tcc-denied');
  });

  test('no-window-found stderr is permanent', () => {
    const r = classifyInjectResult({ rc: 1, stderr: 'no-window-found for role wren' });
    expect(r.kind).toBe('permanent');
    expect(r.reason).toBe('no-window-found');
  });

  test('window-ambiguous stderr is permanent', () => {
    const r = classifyInjectResult({ rc: 1, stderr: 'window-ambiguous: 3 candidates' });
    expect(r.kind).toBe('permanent');
    expect(r.reason).toBe('window-ambiguous');
  });

  test('encoding-error stderr is permanent', () => {
    const r = classifyInjectResult({ rc: 2, stderr: 'encoding-error on input' });
    expect(r.kind).toBe('permanent');
    expect(r.reason).toBe('encoding-error');
  });

  test('unknown stderr is transient', () => {
    const r = classifyInjectResult({ rc: 1, stderr: 'something flaky happened' });
    expect(r.kind).toBe('transient');
    expect(r.reason).toContain('something flaky');
  });

  test('PERMANENT_REASONS set has the expected four reasons', () => {
    expect(PERMANENT_REASONS.has('tcc-denied')).toBe(true);
    expect(PERMANENT_REASONS.has('no-window-found')).toBe(true);
    expect(PERMANENT_REASONS.has('window-ambiguous')).toBe(true);
    expect(PERMANENT_REASONS.has('encoding-error')).toBe(true);
    expect(PERMANENT_REASONS.size).toBe(4);
  });
});

describe('DeliveryWorker enqueue → success path', () => {
  test('success on first try → markDelivered + nudge.surfaced emitted', async () => {
    expect(DeliveryWorker.prototype.enqueue).toBeDefined();
    const id = store.sendNudge('silas', 'wren', 'hello');
    const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
    const worker = new DeliveryWorker(
      store,
      async () => ({ rc: 0, stderr: '' }),
      async (event, fields) => { events.push({ event, fields }); },
      [10, 20],
      async () => { /* no real sleep in test */ },
    );
    await worker.enqueue(rowFor(id));
    const rec = store.getDeliveryRecord(id);
    expect(rec.delivery_status).toBe('delivered');
    expect(events).toHaveLength(1);
    expect(events[0].event).toBe('nudge.surfaced');
    expect(events[0].fields.id).toBe(id);
    expect(events[0].fields.to).toBe('wren');
    expect(events[0].fields.attempt).toBe(1);
  });
});

describe('DeliveryWorker permanent failure path', () => {
  test('tcc-denied on first try → markFailed + nudge.surface.failed permanent=true, no retry', async () => {
    expect(DeliveryWorker.prototype.enqueue).toBeDefined();
    const id = store.sendNudge('silas', 'wren', 'hello');
    let injectCalls = 0;
    const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
    const worker = new DeliveryWorker(
      store,
      async () => { injectCalls++; return { rc: 1, stderr: 'tcc-denied' }; },
      async (event, fields) => { events.push({ event, fields }); },
      [10, 20],
      async () => { /* no real sleep */ },
    );
    await worker.enqueue(rowFor(id));
    expect(injectCalls).toBe(1);
    const rec = store.getDeliveryRecord(id);
    expect(rec.delivery_status).toBe('failed');
    expect(rec.last_delivery_error).toBe('tcc-denied');
    expect(events).toHaveLength(1);
    expect(events[0].event).toBe('nudge.surface.failed');
    expect(events[0].fields.permanent).toBe(true);
    expect(events[0].fields.reason).toBe('tcc-denied');
  });
});

describe('DeliveryWorker transient failure → retry → success', () => {
  test('two transient failures then success → 3 inject calls, 2 surface.failed, 1 surfaced, delivered', async () => {
    expect(DeliveryWorker.prototype.enqueue).toBeDefined();
    const id = store.sendNudge('silas', 'wren', 'hello');
    let injectCalls = 0;
    const responses: InjectResult[] = [
      { rc: 1, stderr: 'flaky' },
      { rc: 1, stderr: 'still flaky' },
      { rc: 0, stderr: '' },
    ];
    const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
    const worker = new DeliveryWorker(
      store,
      async () => responses[injectCalls++],
      async (event, fields) => { events.push({ event, fields }); },
      [10, 20, 40],
      async () => { /* no real sleep */ },
    );
    await worker.enqueue(rowFor(id));
    expect(injectCalls).toBe(3);
    const rec = store.getDeliveryRecord(id);
    expect(rec.delivery_status).toBe('delivered');
    const eventTypes = events.map(e => e.event);
    expect(eventTypes).toEqual(['nudge.surface.failed', 'nudge.surface.failed', 'nudge.surfaced']);
    expect(events[2].fields.attempt).toBe(3);
  });
});

describe('DeliveryWorker exhausted retries', () => {
  test('all attempts transient-fail → markFailed + final surface.failed permanent=false', async () => {
    expect(DeliveryWorker.prototype.enqueue).toBeDefined();
    const id = store.sendNudge('silas', 'wren', 'hello');
    let injectCalls = 0;
    const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
    const backoff = [10, 20]; // 2 backoff slots = 3 max attempts
    const worker = new DeliveryWorker(
      store,
      async () => { injectCalls++; return { rc: 1, stderr: 'always flaky' }; },
      async (event, fields) => { events.push({ event, fields }); },
      backoff,
      async () => { /* no real sleep */ },
    );
    await worker.enqueue(rowFor(id));
    expect(injectCalls).toBe(3);
    const rec = store.getDeliveryRecord(id);
    expect(rec.delivery_status).toBe('failed');
    expect(events).toHaveLength(3);
    expect(events.every(e => e.event === 'nudge.surface.failed')).toBe(true);
    expect(events.every(e => e.fields.permanent === false)).toBe(true);
  });
});

describe('DeliveryWorker per-receiver-role serial FIFO (AC10)', () => {
  test('two enqueues to same receiver are sequential', async () => {
    expect(DeliveryWorker.prototype.enqueue).toBeDefined();
    const id1 = store.sendNudge('silas', 'wren', 'first');
    const id2 = store.sendNudge('silas', 'wren', 'second');
    const callOrder: number[] = [];
    let inFlight = 0;
    let maxConcurrent = 0;
    const worker = new DeliveryWorker(
      store,
      async (_to: string, content: string) => {
        inFlight++;
        if (inFlight > maxConcurrent) maxConcurrent = inFlight;
        await new Promise(r => setTimeout(r, 20));
        callOrder.push(content === 'first' ? id1 : id2);
        inFlight--;
        return { rc: 0, stderr: '' };
      },
      async () => { /* noop */ },
      [10],
      async () => { /* no real sleep */ },
    );
    const p1 = worker.enqueue(rowFor(id1, 'wren', 'first'));
    const p2 = worker.enqueue(rowFor(id2, 'wren', 'second'));
    await Promise.all([p1, p2]);
    expect(callOrder).toEqual([id1, id2]);
    expect(maxConcurrent).toBe(1); // serial — no overlap
  });

  test('two enqueues to different receivers run in parallel', async () => {
    expect(DeliveryWorker.prototype.enqueue).toBeDefined();
    const id1 = store.sendNudge('silas', 'wren', 'to-wren');
    const id2 = store.sendNudge('silas', 'kade', 'to-kade');
    let inFlight = 0;
    let maxConcurrent = 0;
    const worker = new DeliveryWorker(
      store,
      async () => {
        inFlight++;
        if (inFlight > maxConcurrent) maxConcurrent = inFlight;
        await new Promise(r => setTimeout(r, 20));
        inFlight--;
        return { rc: 0, stderr: '' };
      },
      async () => { /* noop */ },
      [10],
      async () => { /* no real sleep */ },
    );
    const p1 = worker.enqueue(rowFor(id1, 'wren'));
    const p2 = worker.enqueue(rowFor(id2, 'kade'));
    await Promise.all([p1, p2]);
    expect(maxConcurrent).toBe(2); // parallel — different receivers overlap
  });
});

describe('DeliveryWorker scanAndRequeue (AC8)', () => {
  test('scans pending rows from store and enqueues each', async () => {
    expect(DeliveryWorker.prototype.scanAndRequeue).toBeDefined();
    const a = store.sendNudge('silas', 'wren', 'a');
    const b = store.sendNudge('silas', 'wren', 'b');
    const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
    const worker = new DeliveryWorker(
      store,
      async () => ({ rc: 0, stderr: '' }),
      async (event, fields) => { events.push({ event, fields }); },
      [10],
      async () => { /* no real sleep */ },
    );
    await worker.scanAndRequeue();
    await new Promise(r => setTimeout(r, 50));
    const recA = store.getDeliveryRecord(a);
    const recB = store.getDeliveryRecord(b);
    expect(recA.delivery_status).toBe('delivered');
    expect(recB.delivery_status).toBe('delivered');
    expect(events.filter(e => e.event === 'nudge.surfaced')).toHaveLength(2);
  });
});

describe('DEFAULT_BACKOFF_MS exported', () => {
  test('matches the AC2 schedule (5 attempts: 250ms→5s)', () => {
    expect(DEFAULT_BACKOFF_MS.length).toBe(5);
    expect(DEFAULT_BACKOFF_MS).toEqual([250, 500, 1000, 2000, 5000]);
  });
});

describe('DeliveryWorker startupSmoke (#2727 AC12)', () => {
  test('selfTest rc=0 → resolves + emits nudge.health.smoke_ok', async () => {
    expect(DeliveryWorker.prototype.startupSmoke).toBeDefined();
    const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
    const worker = new DeliveryWorker(
      store,
      async () => ({ rc: 0, stderr: '' }),
      async (event, fields) => { events.push({ event, fields }); },
      [10],
      async () => { /* no real sleep */ },
      async () => ({ rc: 0, stderr: '' }), // selfTest pass
    );
    await worker.startupSmoke();
    expect(events).toHaveLength(1);
    expect(events[0].event).toBe('nudge.health.smoke_ok');
  });

  test('selfTest rc≠0 → throws + emits nudge.health.smoke_failed', async () => {
    expect(DeliveryWorker.prototype.startupSmoke).toBeDefined();
    const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
    const worker = new DeliveryWorker(
      store,
      async () => ({ rc: 0, stderr: '' }),
      async (event, fields) => { events.push({ event, fields }); },
      [10],
      async () => { /* no real sleep */ },
      async () => ({ rc: 1, stderr: 'tcc-denied at boot' }),
    );
    await expect(worker.startupSmoke()).rejects.toThrow(/startup smoke failed.*tcc-denied/);
    expect(events).toHaveLength(1);
    expect(events[0].event).toBe('nudge.health.smoke_failed');
    expect(events[0].fields.rc).toBe(1);
  });
});
