// @test-type: unit
// @domain: messages
// @card: #4749
// @owner: abby-normal
/**
 * #4749 — pulse GET /api/messages supports ?before=<created_at> for backward history pagination
 */
import request from 'supertest';
import { MessageStore } from './store';
import { createApp } from './service';

function world() {
  const store = new MessageStore(':memory:');
  return { app: createApp(store), store };
}

describe('#4749 GET /api/messages pagination with ?before', () => {
  test('returns only messages created strictly before the specified timestamp', async () => {
    const { app, store } = world();
    store.db.prepare('INSERT INTO messages (type, "from", "to", content, created_at) VALUES (\'nudge\', \'silas\', \'wren\', \'m1\', \'2026-10-26 08:00:00\')').run();
    store.db.prepare('INSERT INTO messages (type, "from", "to", content, created_at) VALUES (\'nudge\', \'silas\', \'wren\', \'m2\', \'2026-10-26 08:05:00\')').run();
    store.db.prepare('INSERT INTO messages (type, "from", "to", content, created_at) VALUES (\'nudge\', \'silas\', \'wren\', \'m3\', \'2026-10-26 08:10:00\')').run();

    // Query before 08:05:00 should return only m1
    const res = await request(app).get('/api/messages').query({ before: '2026-10-26 08:05:00' });
    expect(res.status).toBe(200);
    const msgs = res.body as Array<{ content: string; created_at: string }>;
    expect(msgs).toHaveLength(1);
    expect(msgs[0].content).toBe('m1');
  });

  test('NEGATIVE PROOF: without ?before, all messages are returned up to limit', async () => {
    const { app, store } = world();
    store.db.prepare('INSERT INTO messages (type, "from", "to", content, created_at) VALUES (\'nudge\', \'silas\', \'wren\', \'m1\', \'2026-10-26 08:00:00\')').run();
    store.db.prepare('INSERT INTO messages (type, "from", "to", content, created_at) VALUES (\'nudge\', \'silas\', \'wren\', \'m2\', \'2026-10-26 08:05:00\')').run();

    const res = await request(app).get('/api/messages');
    expect(res.status).toBe(200);
    expect(res.body).toHaveLength(2);
  });

  test('queryMessages filters by pair in both directions', async () => {
    const { app, store } = world();
    store.db.prepare('INSERT INTO messages (type, "from", "to", content, created_at) VALUES (\'nudge\', \'jeff\', \'kade\', \'m1\', \'2026-10-26 08:00:00\')').run();
    store.db.prepare('INSERT INTO messages (type, "from", "to", content, created_at) VALUES (\'nudge\', \'kade\', \'jeff\', \'m2\', \'2026-10-26 08:05:00\')').run();
    store.db.prepare('INSERT INTO messages (type, "from", "to", content, created_at) VALUES (\'nudge\', \'silas\', \'kade\', \'m3\', \'2026-10-26 08:10:00\')').run();

    const res = await request(app).get('/api/messages').query({ pair: 'jeff,kade' });
    expect(res.status).toBe(200);
    const msgs = res.body as Array<{ content: string }>;
    expect(msgs).toHaveLength(2);
    expect(msgs.map((m) => m.content).sort()).toEqual(['m1', 'm2'].sort());
  });
});
