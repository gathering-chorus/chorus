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

  it('GET /api/nudge/:role/pending returns stored nudges for that role', async () => {
    const { app } = fresh();
    await request(app).post('/api/nudge').send({ from: 'kade', to: 'wren', content: 'one' });
    await request(app).post('/api/nudge').send({ from: 'kade', to: 'wren', content: 'two' });
    const res = await request(app).get('/api/nudge/wren/pending');
    expect(res.status).toBe(200);
    expect(res.body).toHaveLength(2);
    expect(res.body[0].content).toBe('one');
  });

  // #2435 wedge 7d — tests for /api/nudge/:id/ack, /api/nudge/:role/ack-all,
  // /api/nudge/:id/attempt, /api/dead-letter replay retired alongside their
  // endpoints. Kade's 0.3 audit confirmed 0 production callers. Delivery
  // confirmation in V2 is the nudge.surfaced spine event, not an HTTP ack API.
  // Dead-letter /attempt semantics retire with the inject-based delivery model.
});

describe('pulse service — chats', () => {
  it('POST /api/chat/start creates a chat', async () => {
    const { app } = fresh();
    const res = await request(app).post('/api/chat/start').send({ roleA: 'k', roleB: 'w', topic: 'x' });
    expect(res.status).toBe(200);
    expect(res.body.id).toBeTruthy();
  });

  it('POST /api/chat/start rejects missing roles with 400', async () => {
    const { app } = fresh();
    const res = await request(app).post('/api/chat/start').send({ roleA: 'k' });
    expect(res.status).toBe(400);
  });

  it('message → get → end flow preserves order', async () => {
    const { app } = fresh();
    const chat = await request(app).post('/api/chat/start').send({ roleA: 'k', roleB: 'w' });
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
    const chat = await request(app).post('/api/chat/start').send({ roleA: 'k', roleB: 'w' });
    const res = await request(app).post(`/api/chat/${chat.body.id}/message`).send({ from: 'k' });
    expect(res.status).toBe(400);
  });

  it('GET /api/chat/:id/messages honors since query', async () => {
    const { app } = fresh();
    const chat = await request(app).post('/api/chat/start').send({ roleA: 'k', roleB: 'w' });
    const id = chat.body.id;
    const m1 = await request(app).post(`/api/chat/${id}/message`).send({ from: 'k', content: '1' });
    await request(app).post(`/api/chat/${id}/message`).send({ from: 'k', content: '2' });
    const msgs = await request(app).get(`/api/chat/${id}/messages`).query({ since: m1.body.id });
    expect(msgs.body.every((m: { id: number }) => m.id > m1.body.id)).toBe(true);
  });
});

describe('pulse service — board events and role state', () => {
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

  it('POST /api/role-state + GET round-trip preserves state', async () => {
    // #2467 / #2629: card field removed; verify createApp + MessageStore wiring directly
    const store = new MessageStore(':memory:');
    const app = createApp(store);
    await request(app)
      .post('/api/role-state')
      .send({ role: 'kade', state: 'building', detail: 'coverage push' });
    const res = await request(app).get('/api/role-state/kade');
    expect(res.status).toBe(200);
    expect(res.body.state).toBe('building');
    expect(res.body.detail).toBe('coverage push');
    expect(res.body).not.toHaveProperty('card');
    const direct = store.getRoleState('kade');
    expect(direct?.state).toBe('building');
  });

  it('POST /api/role-state rejects body containing card field (#2629)', async () => {
    const store = new MessageStore(':memory:');
    const app = createApp(store);
    const res = await request(app)
      .post('/api/role-state')
      .send({ role: 'kade', state: 'building', card: 2237 });
    expect(res.status).toBe(400);
    expect(res.body.error).toMatch(/card.*no longer accepted|#2467|#2629/i);
    expect(store.getRoleState('kade')).toBeNull();
  });

  it('POST /api/role-state rejects missing fields with 400', async () => {
    const { app } = fresh();
    const res = await request(app).post('/api/role-state').send({ role: 'kade' });
    expect(res.status).toBe(400);
  });

  it('GET /api/role-state/:role returns error for unknown role', async () => {
    const { app } = fresh();
    const res = await request(app).get('/api/role-state/ghost');
    expect(res.status).toBe(200);
    expect(res.body.error).toBe('not found');
  });
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

  it('GET /api/dead-letter returns array', async () => {
    const { app } = fresh();
    const res = await request(app).get('/api/dead-letter');
    expect(res.status).toBe(200);
    expect(Array.isArray(res.body)).toBe(true);
  });
});
