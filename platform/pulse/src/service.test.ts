/**
 * service.test.ts — REST contract for pulse messaging service (#2237)
 *
 * Covers every endpoint exposed by createApp() against an in-memory SQLite
 * MessageStore. Tests describe what a CLI client (nudge, chat.sh, role-state)
 * sees when it calls the API, not internal store behavior.
 */

import request from 'supertest';
import { MessageStore } from './store';
import { createApp } from './service';

const CHAT_START = '/api/chat/start';

// #2804 — pulse rejects POST /api/nudge without X-Chorus-MCP-Caller header.
// Tests bypass via PULSE_ALLOW_DIRECT_POST=1 set in beforeAll. One explicit
// test below verifies the gate (rejection on missing header).
beforeAll(() => {
  process.env.PULSE_ALLOW_DIRECT_POST = '1';
});
afterAll(() => {
  delete process.env.PULSE_ALLOW_DIRECT_POST;
});

function fresh() {
  const store = new MessageStore(':memory:');
  const app = createApp(store);
  return { app, store };
}

describe('pulse service — health and metrics', () => {
  it('GET /health returns ok with port and stats', async () => {
    const { app } = fresh();
    const res = await request(app).get('/health');
    expect(res.status).toBe(200);
    expect(res.body.status).toBe('ok');
    expect(res.body).toHaveProperty('pending');
  });

  it('GET /metrics returns prometheus exposition format', async () => {
    const { app } = fresh();
    const res = await request(app).get('/metrics');
    expect(res.status).toBe(200);
    expect(res.text).toContain('messaging_');
  });
});

describe('#2804 pulse caller-check', () => {
  it('POST /api/nudge without X-Chorus-MCP-Caller header is rejected with 403 + typed error', async () => {
    expect(MessageStore.prototype.sendNudge).toBeDefined();
    const { app } = fresh();
    delete process.env.PULSE_ALLOW_DIRECT_POST;
    try {
      const res = await request(app)
        .post('/api/nudge')
        .send({ from: 'k', to: 'w', content: 'hi' });
      expect(res.status).toBe(403);
      expect(res.body.error).toBe('not-canonical-caller');
      expect(res.body.message).toMatch(/chorus_nudge_message MCP/);
    } finally {
      process.env.PULSE_ALLOW_DIRECT_POST = '1';
    }
  });

  it('POST /api/nudge with X-Chorus-MCP-Caller header is accepted', async () => {
    expect(MessageStore.prototype.sendNudge).toBeDefined();
    const { app } = fresh();
    delete process.env.PULSE_ALLOW_DIRECT_POST;
    try {
      const res = await request(app)
        .post('/api/nudge')
        .set('X-Chorus-MCP-Caller', '1')
        .send({ from: 'k', to: 'w', content: 'hi' });
      expect(res.status).toBe(200);
      expect(res.body.ok).toBe(true);
    } finally {
      process.env.PULSE_ALLOW_DIRECT_POST = '1';
    }
  });
});

describe('pulse service — nudges', () => {
  it('POST /api/nudge with full payload returns id', async () => {
    const { app } = fresh();
    const res = await request(app)
      .post('/api/nudge')
      .send({ from: 'kade', to: 'wren', content: 'hello', traceId: 't-1' });
    expect(res.status).toBe(200);
    expect(res.body.ok).toBe(true);
    expect(typeof res.body.id).toBe('number');
    expect(res.body.traceId).toBe('t-1');
  });

  it('POST /api/nudge rejects missing fields with 400', async () => {
    const { app } = fresh();
    const res = await request(app).post('/api/nudge').send({ from: 'kade' });
    expect(res.status).toBe(400);
    expect(res.body.error).toMatch(/from, to, content required/);
  });

  // #2664: retirement assertion — GET /api/nudge/:role/pending is gone.
  // Delivery confirmation is the nudge.surfaced spine event, not HTTP read.
  it('GET /api/nudge/:role/pending returns 404 (retired #2664)', async () => {
    const { app } = fresh();
    await request(app).post('/api/nudge').send({ from: 'kade', to: 'wren', content: 'one' });
    const res = await request(app).get('/api/nudge/wren/pending');
    expect(res.status).toBe(404);
  });

  // #2435 wedge 7d — tests for /api/nudge/:id/ack, /api/nudge/:role/ack-all,
  // /api/nudge/:id/attempt retired alongside their endpoints.
});

describe('pulse service — chats', () => {
  it('POST /api/chat/start creates a chat', async () => {
    const { app } = fresh();
    const res = await request(app).post(CHAT_START).send({ roleA: 'k', roleB: 'w', topic: 'x' });
    expect(res.status).toBe(200);
    expect(res.body.id).toBeTruthy();
  });

  it('POST /api/chat/start rejects missing roles with 400', async () => {
    const { app } = fresh();
    const res = await request(app).post(CHAT_START).send({ roleA: 'k' });
    expect(res.status).toBe(400);
  });

  it('message → get → end flow preserves order', async () => {
    const { app } = fresh();
    const chat = await request(app).post(CHAT_START).send({ roleA: 'k', roleB: 'w' });
    const id = chat.body.id;
    await request(app).post(`/api/chat/${id}/message`).send({ from: 'k', content: 'hi' });
    await request(app).post(`/api/chat/${id}/message`).send({ from: 'w', content: 'yo' });
    const msgs = await request(app).get(`/api/chat/${id}/messages`);
    expect(msgs.status).toBe(200);
    expect(msgs.body.length).toBeGreaterThanOrEqual(2);
    const end = await request(app).post(`/api/chat/${id}/end`);
    expect(end.status).toBe(200);
  });

  it('POST /api/chat/:id/message rejects missing fields with 400', async () => {
    const { app } = fresh();
    const chat = await request(app).post(CHAT_START).send({ roleA: 'k', roleB: 'w' });
    const res = await request(app).post(`/api/chat/${chat.body.id}/message`).send({ from: 'k' });
    expect(res.status).toBe(400);
  });

  it('GET /api/chat/:id/messages honors since query', async () => {
    const { app } = fresh();
    const chat = await request(app).post(CHAT_START).send({ roleA: 'k', roleB: 'w' });
    const id = chat.body.id;
    const m1 = await request(app).post(`/api/chat/${id}/message`).send({ from: 'k', content: '1' });
    await request(app).post(`/api/chat/${id}/message`).send({ from: 'k', content: '2' });
    const msgs = await request(app).get(`/api/chat/${id}/messages`).query({ since: m1.body.id });
    expect(msgs.body.every((m: { id: number }) => m.id > m1.body.id)).toBe(true);
  });
});

describe('pulse service — board events', () => {
  it('POST /api/board-event persists board event', async () => {
    const { app } = fresh();
    const res = await request(app).post('/api/board-event').send({ from: 'kade', content: 'moved #1 to WIP' });
    expect(res.status).toBe(200);
    expect(typeof res.body.id).toBe('number');
  });

  it('POST /api/board-event rejects missing fields with 400', async () => {
    const { app } = fresh();
    const res = await request(app).post('/api/board-event').send({ from: 'kade' });
    expect(res.status).toBe(400);
  });

  // #2632: /api/role-state tests retired alongside the endpoint.
});

describe('pulse service — queries and dead letter', () => {
  it('GET /api/messages returns filtered messages', async () => {
    const { app } = fresh();
    await request(app).post('/api/nudge').send({ from: 'k', to: 'w', content: 'x' });
    const res = await request(app).get('/api/messages').query({ type: 'nudge', limit: 10 });
    expect(res.status).toBe(200);
    expect(Array.isArray(res.body)).toBe(true);
    expect(res.body.length).toBeGreaterThan(0);
  });

  it('GET /api/stats returns counter object', async () => {
    const { app } = fresh();
    const res = await request(app).get('/api/stats');
    expect(res.status).toBe(200);
    expect(res.body).toHaveProperty('pending');
  });

  // #2664: retirement assertion — GET /api/dead-letter is gone alongside
  // POST /api/dead-letter/:id/replay. Inject-watcher (the writer that
  // produced dead-letter state via recordDeliveryAttempt) was retired by
  // #2435. Zero production callers per Kade's 0.3 audit.
  it('GET /api/dead-letter returns 404 (retired #2664)', async () => {
    const { app } = fresh();
    const res = await request(app).get('/api/dead-letter');
    expect(res.status).toBe(404);
  });
});

// =================================================================
// #2766 E2E — pulse-owns-delivery contract end-to-end
// =================================================================
// These tests exercise the full pulse stack: real Express, real store,
// real DeliveryWorker — only chorus-inject is mocked. They cover the
// three lifecycle scenarios from #2727 AC5: happy-path, retry, hard-fail.
// Real osascript / TCC behavior is not in scope (manual demo verification);
// these prove the contract holds at the pulse + worker boundary.

import { DeliveryWorker, type InjectResult } from './delivery-worker';

function freshWithWorker(injectResponses: InjectResult[]) {
  const store = new MessageStore(':memory:');
  let injectIdx = 0;
  const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
  const runInject = async (): Promise<InjectResult> => {
    const r = injectResponses[injectIdx % injectResponses.length];
    injectIdx++;
    return r;
  };
  const emitSpine = async (event: string, fields: Record<string, unknown>): Promise<void> => {
    events.push({ event, fields });
  };
  const worker = new DeliveryWorker(
    store,
    runInject,
    emitSpine,
    [10, 20, 40, 80, 160], // accelerated backoff for tests
    async () => { /* no real sleep */ },
  );
  const app = createApp(store, worker);
  return { app, store, worker, events, getInjectCalls: () => injectIdx };
}

async function waitForState(store: MessageStore, id: number, expected: string, timeoutMs = 500): Promise<void> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    const rec = store.getDeliveryRecord(id);
    if (rec.delivery_status === expected) return;
    await new Promise(r => setTimeout(r, 5));
  }
  const rec = store.getDeliveryRecord(id);
  throw new Error(`waitForState: expected ${expected}, got ${rec.delivery_status} after ${timeoutMs}ms`);
}

describe('#2766 E2E — pulse-owns-delivery lifecycle', () => {
  it('AC1 happy-path: POST /api/nudge → row delivered, nudge.surfaced emitted', async () => {
    expect(MessageStore.prototype.getDeliveryRecord).toBeDefined();
    expect(DeliveryWorker.prototype.enqueue).toBeDefined();
    const { app, store, events } = freshWithWorker([{ rc: 0, stderr: '' }]);
    const res = await request(app)
      .post('/api/nudge')
      .set('X-Chorus-Trace-Id', '018f-happy-trace')
      .send({ from: 'silas', to: 'wren', content: 'happy nudge' });
    expect(res.status).toBe(200);
    const id = res.body.id;
    await waitForState(store, id, 'delivered');
    const rec = store.getDeliveryRecord(id);
    expect(rec.delivery_status).toBe('delivered');
    expect(rec.delivered_at).not.toBeNull();
    expect(rec.last_delivery_error).toBeNull();
    expect(rec.trace_id).toBe('018f-happy-trace');
    const surfacedEvents = events.filter(e => e.event === 'nudge.surfaced');
    expect(surfacedEvents).toHaveLength(1);
    expect(surfacedEvents[0].fields.trace_id).toBe('018f-happy-trace');
    expect(surfacedEvents[0].fields.attempt).toBe(1);
  });

  it('AC2 retry: 2 transient failures then success → 3 inject calls, 2 surface.failed events, 1 surfaced', async () => {
    expect(DeliveryWorker.prototype.enqueue).toBeDefined();
    const { app, store, events, getInjectCalls } = freshWithWorker([
      { rc: 1, stderr: 'flaky transient' },
      { rc: 1, stderr: 'still flaky' },
      { rc: 0, stderr: '' },
    ]);
    const res = await request(app)
      .post('/api/nudge')
      .set('X-Chorus-Trace-Id', '018f-retry-trace')
      .send({ from: 'silas', to: 'wren', content: 'retry nudge' });
    expect(res.status).toBe(200);
    const id = res.body.id;
    await waitForState(store, id, 'delivered');
    expect(getInjectCalls()).toBe(3);
    const failedEvents = events.filter(e => e.event === 'nudge.surface.failed');
    const surfacedEvents = events.filter(e => e.event === 'nudge.surfaced');
    expect(failedEvents).toHaveLength(2);
    expect(failedEvents.every(e => e.fields.permanent === false)).toBe(true);
    expect(surfacedEvents).toHaveLength(1);
    expect(surfacedEvents[0].fields.attempt).toBe(3);
  });

  it('AC3 hard-fail: permanent reason on first try → markFailed + nudge.surface.failed permanent=true, no retry', async () => {
    expect(DeliveryWorker.prototype.enqueue).toBeDefined();
    const { app, store, events, getInjectCalls } = freshWithWorker([
      { rc: 1, stderr: 'no claude window found for wren (looking for wren + claude)' },
    ]);
    const res = await request(app)
      .post('/api/nudge')
      .set('X-Chorus-Trace-Id', '018f-hardfail-trace')
      .send({ from: 'silas', to: 'wren', content: 'hardfail nudge' });
    expect(res.status).toBe(200);
    const id = res.body.id;
    await waitForState(store, id, 'failed');
    expect(getInjectCalls()).toBe(1);
    const rec = store.getDeliveryRecord(id);
    expect(rec.delivery_status).toBe('failed');
    expect(rec.last_delivery_error).toBe('no-window-found');
    const failedEvents = events.filter(e => e.event === 'nudge.surface.failed');
    expect(failedEvents).toHaveLength(1);
    expect(failedEvents[0].fields.permanent).toBe(true);
    expect(failedEvents[0].fields.reason).toBe('no-window-found');
    expect(failedEvents[0].fields.trace_id).toBe('018f-hardfail-trace');
  });

  it('AC3 (variant) exhausted retries: all attempts transient-fail → markFailed permanent=false', async () => {
    expect(DeliveryWorker.prototype.enqueue).toBeDefined();
    const { app, store, events, getInjectCalls } = freshWithWorker([
      { rc: 1, stderr: 'always flaky' },
    ]);
    const res = await request(app)
      .post('/api/nudge')
      .set('X-Chorus-Trace-Id', '018f-exhaust-trace')
      .send({ from: 'silas', to: 'wren', content: 'exhaust nudge' });
    expect(res.status).toBe(200);
    const id = res.body.id;
    await waitForState(store, id, 'failed', 1000);
    expect(getInjectCalls()).toBe(6); // backoff 5 slots + initial = 6 attempts
    const rec = store.getDeliveryRecord(id);
    expect(rec.delivery_status).toBe('failed');
    const failedEvents = events.filter(e => e.event === 'nudge.surface.failed');
    expect(failedEvents).toHaveLength(6);
    expect(failedEvents.every(e => e.fields.permanent === false)).toBe(true);
  });

  it('trace_id without header falls back to body.traceId; without either, row.trace_id is null', async () => {
    expect(MessageStore.prototype.getDeliveryRecord).toBeDefined();
    const { app, store } = freshWithWorker([{ rc: 0, stderr: '' }]);
    const r1 = await request(app).post('/api/nudge').send({ from: 'k', to: 'w', content: 'a', traceId: 'body-trace' });
    await waitForState(store, r1.body.id, 'delivered');
    expect(store.getDeliveryRecord(r1.body.id).trace_id).toBe('body-trace');
    const r2 = await request(app).post('/api/nudge').send({ from: 'k', to: 'w', content: 'b' });
    await waitForState(store, r2.body.id, 'delivered');
    expect(store.getDeliveryRecord(r2.body.id).trace_id).toBeNull();
  });
});
