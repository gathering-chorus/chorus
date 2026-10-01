// @test-type: unit — POST /drain over HTTP (supertest) against an in-memory store; secret from the test env
// @card: #4417
// @owner: wren
/**
 * #3700 — a role that was busy has its nudges parked as "queued"; when its turn
 * ends, its hook POSTs /drain and pulse releases exactly that role's queue.
 * #4417 — the route had no test. This drives it: who may call it, what a bad
 * role looks like, and that one role's drain never releases another's.
 */
import request from 'supertest';
import { MessageStore } from './store';
import { createApp } from './service';

const SECRET = 'drain-test-secret';
beforeAll(() => { process.env.CHORUS_PULSE_SECRET = SECRET; });
afterAll(() => { delete process.env.CHORUS_PULSE_SECRET; });

function world() {
  const store = new MessageStore(':memory:');
  const app = createApp(store);
  const queue = (to: string, text: string) => {
    const id = store.sendNudge('silas', to, text);
    store.markQueued(id, 'target-busy');
    return id;
  };
  const status = (id: number) => (store as unknown as { db: { prepare: (q: string) => { get: (i: number) => { delivery_status: string } } } })
    .db.prepare('SELECT delivery_status FROM messages WHERE id = ?').get(id).delivery_status;
  return { app, queue, status };
}

describe('#3700 POST /drain', () => {
  test('a role\'s queued nudges are released, in full, and only that role\'s', async () => {
    const { app, queue, status } = world();
    const a = queue('wren', 'one');
    const b = queue('wren', 'two');
    const k = queue('kade', 'not yours');
    const r = await request(app).post('/drain').set('x-chorus-pulse-secret', SECRET).send({ role: 'wren' });
    expect(r.status).toBe(200);
    expect(r.body).toEqual({ ok: true, role: 'wren', released: 2 });
    expect([status(a), status(b)]).toEqual(['pending', 'pending']);
    expect(status(k)).toBe('queued');
  });

  test('NEGATIVE PROOF: without the shared secret → 403, and nothing is released', async () => {
    const { app, queue, status } = world();
    const a = queue('wren', 'held');
    const r = await request(app).post('/drain').set('x-chorus-pulse-secret', 'wrong').send({ role: 'wren' });
    expect(r.status).toBe(403);
    expect(status(a)).toBe('queued');
    const none = await request(app).post('/drain').send({ role: 'wren' });
    expect(none.status).toBe(403);
  });

  test.each([[''], ['Wren'], ['wren; drop'], ['x'.repeat(40)]])('a bad role (%p) → 400', async (role) => {
    const { app } = world();
    const r = await request(app).post('/drain').set('x-chorus-pulse-secret', SECRET).send({ role });
    expect(r.status).toBe(400);
  });

  test('a role with nothing queued → released 0', async () => {
    const { app } = world();
    const r = await request(app).post('/drain').set('x-chorus-pulse-secret', SECRET).send({ role: 'silas' });
    expect(r.body).toEqual({ ok: true, role: 'silas', released: 0 });
  });
});
