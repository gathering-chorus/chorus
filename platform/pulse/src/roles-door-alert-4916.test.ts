// @test-type: unit — in-memory store, peers lookup mocked to fail with Discovery fixture
// @domain: messages
// @card: #4916
// @owner: silas
/**
 * #4916 — an alert about the roles door, or about delivery itself, must reach its owner
 * without going through the roles door (when the door is dead, its alarm cannot ride on it).
 */
import request from 'supertest';
import { MessageStore } from './store';
import { createApp, resetNudgeDedup } from './service';

// Door answers Discovery fixture (no data list of roles)
jest.mock('./peers', () => ({
  fetchPeers: () => Promise.reject(new Error('the roles door answered with no data list')),
  startRolesDoorWatch: () => () => {},
}));

describe('#4916 roles door alert reaches owner without using the door', () => {
  let store: MessageStore;
  let app: ReturnType<typeof createApp>;

  beforeEach(() => {
    resetNudgeDedup();
    store = new MessageStore(':memory:');
    process.env.PULSE_ALLOW_DIRECT_POST = '1';
    app = createApp(store);
  });

  test('regular peer nudge is refused with 503 when roles door answers Discovery', async () => {
    const res = await request(app).post('/api/nudge').send({
      from: 'wren',
      to: 'kade',
      content: 'hello',
      expects: 'reply',
    });
    expect(res.status).toBe(503);
    expect(res.body.error).toMatch(/roles door unreadable/);
    expect(store.getStats().total).toBe(0);
  });

  test('NEGATIVE PROOF: door down (Discovery fixture), and the door alert still lands for Silas', async () => {
    const res = await request(app).post('/api/nudge').send({
      from: 'system',
      to: 'silas',
      content: 'grafana: Roles door: pulse could not read the peer list — the roles door answered with no data list',
    });
    expect(res.status).toBe(200);
    expect(res.body.ok).toBe(true);
    expect(res.body.id).toBeDefined();
    expect(store.getStats().total).toBe(1);
    const msg = store.getMessageEnds(res.body.id);
    expect(msg?.to).toBe('silas');
    expect(msg?.content).toContain('[alert for silas · from system');
  });

  test('NEGATIVE PROOF: door down (Discovery fixture), and a delivery alert still lands for Silas', async () => {
    const res = await request(app).post('/api/nudge').send({
      from: 'ops',
      to: 'silas',
      content: 'alert delivery failed: connection reset',
    });
    expect(res.status).toBe(200);
    expect(res.body.ok).toBe(true);
    expect(res.body.id).toBeDefined();
    expect(store.getStats().total).toBe(1);
  });

  test('NEGATIVE PROOF: door down, a role cannot pass itself off by naming a recipient', async () => {
    const res = await request(app).post('/api/nudge').send({ from: 'mallory', to: 'silas', content: 'roles door is down' });
    expect(res.status).toBe(503);
    expect(store.getStats().total).toBe(0);
  });
});
