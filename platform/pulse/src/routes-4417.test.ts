// @test-type: unit — pulse's chat, board-event, messages and stats routes over HTTP (supertest) against an in-memory store
// @domain: messages — placed by chorus-crawl's domain rules (#4538)
// @card: #4417
// @owner: wren
/**
 * #4417 — the thin pulse routes from Kade's map (10-01 12:23). Role-to-role chat
 * (chat.sh) used to store a message for any id: to a chat that did not exist
 * (addressed to "unknown"), after it ended, or from a role not in it.
 */
import request from 'supertest';
import { MessageStore } from './store';
import { createApp } from './service';

function world() {
  const store = new MessageStore(':memory:');
  return { app: createApp(store), store };
}

describe('#4417 role-to-role chat', () => {
  test('start → two roles talk → the transcript holds both, addressed to each other → end', async () => {
    const { app } = world();
    const { id } = (await request(app).post('/api/chat/start').send({ roleA: 'wren', roleB: 'kade', topic: 't' })).body;
    expect((await request(app).post(`/api/chat/${id}/message`).send({ from: 'wren', content: 'hi kade' })).status).toBe(200);
    expect((await request(app).post(`/api/chat/${id}/message`).send({ from: 'kade', content: 'hi wren' })).status).toBe(200);
    const msgs = (await request(app).get(`/api/chat/${id}/messages`)).body as Array<{ from: string; to: string; content: string }>;
    expect(msgs.map((m) => [m.from, m.to, m.content])).toEqual([['wren', 'kade', 'hi kade'], ['kade', 'wren', 'hi wren']]);
    expect((await request(app).post(`/api/chat/${id}/end`)).status).toBe(200);
  });

  test('NEGATIVE PROOF: a message to a chat that does not exist → 404, nothing stored', async () => {
    const { app } = world();
    const r = await request(app).post('/api/chat/nope/message').send({ from: 'wren', content: 'x' });
    expect(r.status).toBe(404);
    expect((await request(app).get('/api/chat/nope/messages')).body).toEqual([]);
  });

  test('NEGATIVE PROOF: a message after the chat ended → 409, nothing stored', async () => {
    const { app } = world();
    const { id } = (await request(app).post('/api/chat/start').send({ roleA: 'wren', roleB: 'kade' })).body;
    await request(app).post(`/api/chat/${id}/end`);
    expect((await request(app).post(`/api/chat/${id}/message`).send({ from: 'wren', content: 'late' })).status).toBe(409);
    expect((await request(app).get(`/api/chat/${id}/messages`)).body).toEqual([]);
  });

  test('NEGATIVE PROOF: a role that is not in the chat → 403', async () => {
    const { app } = world();
    const { id } = (await request(app).post('/api/chat/start').send({ roleA: 'wren', roleB: 'kade' })).body;
    expect((await request(app).post(`/api/chat/${id}/message`).send({ from: 'silas', content: 'butting in' })).status).toBe(403);
  });

  test('ending a chat that does not exist → 404; starting without both roles → 400', async () => {
    const { app } = world();
    expect((await request(app).post('/api/chat/nope/end')).status).toBe(404);
    expect((await request(app).post('/api/chat/start').send({ roleA: 'wren' })).status).toBe(400);
  });
});

describe('board events, the message query and the counts', () => {
  test('a board event is stored and found by type; missing fields → 400', async () => {
    const { app, store } = world();
    expect((await request(app).post('/api/board-event').send({ from: 'cards' })).status).toBe(400);
    const r = await request(app).post('/api/board-event').send({ from: 'cards', content: '#4417 moved to WIP' });
    expect(r.status).toBe(200);
    const all = store.queryMessages({ from: 'cards' }) as Array<{ content: string }>;
    expect(all.map((m) => m.content)).toContain('#4417 moved to WIP');
  });

  test('the query honours its filters and its limit', () => {
    const { store } = world();
    for (let i = 0; i < 5; i++) store.sendNudge('silas', 'wren', `n${i}`);
    store.sendNudge('kade', 'silas', 'other');
    const toWren = store.queryMessages({ to: 'wren' }) as Array<{ to: string }>;
    expect(toWren).toHaveLength(5);
    expect(toWren.every((m) => m.to === 'wren')).toBe(true);
    expect(store.queryMessages({ to: 'wren', limit: 2 })).toHaveLength(2);
  });

  test('stats count what is stored', async () => {
    const { app, store } = world();
    store.sendNudge('silas', 'wren', 'a');
    store.sendNudge('silas', 'kade', 'b');
    const s = (await request(app).get('/api/stats')).body as { total: number; byType: Record<string, number> };
    expect(s.total).toBe(2);
    expect(s.byType.nudge).toBe(2);
  });
});
