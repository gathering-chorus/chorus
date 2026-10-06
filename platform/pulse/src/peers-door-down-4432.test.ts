// @test-type: unit — in-memory store, peers lookup mocked to fail; no live services
// @domain: messages
/** #4432 — when the roles door can't name the peers, a nudge is refused loudly, never stored as a guess. */
import request from 'supertest';
import { MessageStore } from './store';
import { createApp, resetNudgeDedup } from './service';

jest.mock('./peers', () => ({ fetchPeers: () => Promise.reject(new Error('the roles door answered HTTP 502')) }));

test('NEGATIVE PROOF: door down → 503, nothing stored', async () => {
  resetNudgeDedup();
  const store = new MessageStore(':memory:');
  process.env.PULSE_ALLOW_DIRECT_POST = '1';
  const res = await request(createApp(store)).post('/api/nudge').send({ from: 'abby-normal', to: 'wren', content: 'hi', expects: 'reply' });
  expect(res.status).toBe(503);
  expect(res.body.error).toMatch(/roles door unreadable/);
  expect(store.getStats().total).toBe(0);
});
