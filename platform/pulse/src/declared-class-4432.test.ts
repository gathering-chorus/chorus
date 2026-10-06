// @test-type: unit — in-memory store, peers mocked; no live services
// @domain: messages
/** #4432 — only a peer's nudge can be r2r; a machine that declares r2r is stored a2r. */
import request from 'supertest';
import { MessageStore } from './store';
import { createApp, resetNudgeDedup } from './service';

jest.mock('./peers', () => ({ fetchPeers: () => Promise.resolve(['abby-normal', 'jeff', 'kade', 'silas', 'wren']) }));

async function classOf(from: string, declared?: string): Promise<string> {
  resetNudgeDedup();
  const store = new MessageStore(':memory:');
  process.env.PULSE_ALLOW_DIRECT_POST = '1';
  const res = await request(createApp(store)).post('/api/nudge').send({ from, to: 'silas', content: `x ${from} ${declared}`, class: declared, expects: 'reply' });
  expect(res.status).toBe(200);
  const db = (store as unknown as { db: { prepare(q: string): { get(id: number): { nudge_class: string } } } }).db;
  return db.prepare('SELECT nudge_class FROM messages WHERE id = ?').get(res.body.id as number).nudge_class;
}

test('a nudge from Abby is r2r', async () => { expect(await classOf('abby-normal')).toBe('r2r'); });
test('NEGATIVE PROOF: a machine that declares r2r is stored a2r', async () => { expect(await classOf('system', 'r2r')).toBe('a2r'); });
test('a peer may still declare a2r', async () => { expect(await classOf('wren', 'a2r')).toBe('a2r'); });
