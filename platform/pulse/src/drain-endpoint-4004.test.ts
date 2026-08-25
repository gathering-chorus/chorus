// @test-type: integration — supertest over createApp(); in-memory store, no live pulse (signal:security is the auth-gate subject)
//
// #4004 — POST /drain is the #3700 pull-based last mile: a role's own
// turn-boundary hook releases its queued nudges. Its auth and validation
// branches were the last uncovered lines in service.ts. Untested, the gate
// that keeps an unauthenticated caller from draining someone else's queue is
// only a comment.
import request from 'supertest';
import { createApp } from './service';
import { MessageStore } from './store';

const SECRET = 'drain-secret-4004';

describe('#4004 POST /drain — the queued last mile', () => {
  afterEach(() => { delete process.env.CHORUS_PULSE_SECRET; });

  it('refuses an unauthenticated caller — 403, and nothing is released', async () => {
    process.env.CHORUS_PULSE_SECRET = SECRET;
    const store = new MessageStore(':memory:');
    const app = createApp(store);
    const res = await request(app).post('/drain').send({ role: 'silas' });
    expect(res.status).toBe(403);
    expect(res.body.error).toBe('unauthorized');
  });

  it('refuses a WRONG secret — a near-miss is still a refusal', async () => {
    process.env.CHORUS_PULSE_SECRET = SECRET;
    const store = new MessageStore(':memory:');
    const app = createApp(store);
    const res = await request(app)
      .post('/drain')
      .set('X-Chorus-Pulse-Secret', 'not-the-secret')
      .send({ role: 'silas' });
    expect(res.status).toBe(403);
  });

  it('refuses a malformed role rather than draining something unintended', async () => {
    process.env.CHORUS_PULSE_SECRET = SECRET;
    const store = new MessageStore(':memory:');
    const app = createApp(store);
    const res = await request(app)
      .post('/drain')
      .set('X-Chorus-Pulse-Secret', SECRET)
      .send({ role: '../../etc' });
    expect(res.status).toBe(400);
    expect(res.body.error).toBe('bad role');
  });

  it('refuses a missing role', async () => {
    process.env.CHORUS_PULSE_SECRET = SECRET;
    const store = new MessageStore(':memory:');
    const app = createApp(store);
    const res = await request(app)
      .post('/drain')
      .set('X-Chorus-Pulse-Secret', SECRET)
      .send({});
    expect(res.status).toBe(400);
  });

  it('NEGATIVE PROOF: the authorized caller with a valid role IS served (#3734)', async () => {
    process.env.CHORUS_PULSE_SECRET = SECRET;
    const store = new MessageStore(':memory:');
    const app = createApp(store);
    const res = await request(app)
      .post('/drain')
      .set('X-Chorus-Pulse-Secret', SECRET)
      .send({ role: 'silas' });
    expect(res.status).toBe(200);
    expect(res.body).toMatchObject({ ok: true, role: 'silas' });
    expect(typeof res.body.released).toBe('number');
  });
});
