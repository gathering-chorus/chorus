// @domain: messages
// @test-type: unit — express app over a temp messages.db, Presence from fixtures; no live services
// @card: #4462
// @owner: wren
/**
 * #4462 — the agent inbox's refusals and receipt cases that no test reached:
 * a hook gets a typed answer for every bad call, and reconcile never replays
 * a message the supervisor already admitted.
 */
import * as fs from 'fs';
import * as path from 'path';
import express from 'express';
import request from 'supertest';
import { registerAgentInbox, reconcileAgentInbox } from './agent-inbox';
import { resolveFromPresence } from './presence-target';
import { MessageStore } from './store';
import type { DeliveryWorker } from './delivery-worker';
import type { AgentSupervisor } from './agent-supervisor';

const SECRET = 'test-secret-4462';
const RUN = 'kade-run-live';
const H = 'x-chorus-pulse-secret';
const noop = (): Promise<void> => Promise.resolve();
const runs = [{ name: RUN, runEndedAt: '' }];
const presences = [{ name: 'kade-presence-now', presenceOf: `session-run-${RUN}`, reachableOver: 'agent' }];
const resolve = (role: string) => Promise.resolve(resolveFromPresence(presences, runs, role));

const DB = path.join(__dirname, '..', 'test-agent-inbox-4462.db');
let store: MessageStore;
let savedSecret: string | undefined;
beforeEach(() => {
  savedSecret = process.env.CHORUS_PULSE_SECRET;
  process.env.CHORUS_PULSE_SECRET = SECRET;
  if (fs.existsSync(DB)) fs.unlinkSync(DB);
  store = new MessageStore(DB);
});
afterEach(() => {
  if (savedSecret === undefined) delete process.env.CHORUS_PULSE_SECRET; else process.env.CHORUS_PULSE_SECRET = savedSecret;
  try { store.close(); } catch { /* closed */ }
  if (fs.existsSync(DB)) fs.unlinkSync(DB);
});

function app(r: Parameters<typeof registerAgentInbox>[2] = resolve) {
  const a = express();
  a.use(express.json());
  registerAgentInbox(a, store, r);
  return a;
}

describe('the inbox answers every bad call by name', () => {
  test('a role or run that is not well formed is 400 invalid-recipient', async () => {
    const res = await request(app()).post('/api/agent-inbox/claim').set(H, SECRET).send({ role: 'kade', session_id: 'has spaces' });
    expect(res.status).toBe(400);
    expect(res.body.error).toBe('invalid-recipient');
  });

  test('when Presence cannot be read the claim is 503 presence-unread, never a guess', async () => {
    const unread = () => Promise.resolve({ kind: 'unread' as const, why: 'the door answered 502' });
    const res = await request(app(unread)).post('/api/agent-inbox/claim').set(H, SECRET).send({ role: 'kade', session_id: RUN });
    expect(res.status).toBe(503);
    expect(res.body).toEqual({ error: 'presence-unread', why: 'the door answered 502' });
  });

  test('a limit outside 1..100 is 400 invalid-limit and claims nothing', async () => {
    const id = store.sendNudge('silas', 'kade', 'hello');
    const res = await request(app()).post('/api/agent-inbox/claim').set(H, SECRET).send({ role: 'kade', session_id: RUN, limit: 0 });
    expect(res.status).toBe(400);
    expect(res.body.error).toBe('invalid-limit');
    expect(store.getDeliveryRecord(id).delivery_status).toBe('pending');
  });

  test('ack ids that repeat are 400 invalid-message-ids', async () => {
    const res = await request(app()).post('/api/agent-inbox/ack').set(H, SECRET).send({ role: 'kade', session_id: RUN, ids: [1, 1] });
    expect(res.status).toBe(400);
    expect(res.body.error).toBe('invalid-message-ids');
  });

  test('acking a message this run never claimed is 409 and leaves it undelivered', async () => {
    const id = store.sendNudge('silas', 'kade', 'hello');
    const res = await request(app()).post('/api/agent-inbox/ack').set(H, SECRET).send({ role: 'kade', session_id: RUN, ids: [id] });
    expect(res.status).toBe(409);
    expect(res.body.error).toBe('inbox-claim-mismatch');
    expect(store.getDeliveryRecord(id).delivery_status).toBe('pending');
  });

  test('the ack still answers delivered when the audit sink is down', async () => {
    const id = store.sendNudge('silas', 'kade', 'hello');
    const a = express();
    a.use(express.json());
    registerAgentInbox(a, store, resolve, () => Promise.reject(new Error('spine down')));
    await request(a).post('/api/agent-inbox/claim').set(H, SECRET).send({ role: 'kade', session_id: RUN });
    const ack = await request(a).post('/api/agent-inbox/ack').set(H, SECRET).send({ role: 'kade', session_id: RUN, ids: [id] });
    expect(ack.status).toBe(200);
    expect(store.getDeliveryRecord(id).delivery_status).toBe('delivered');
  });
});

describe('reconcile settles by receipt and offers again only what the supervisor refused', () => {
  function worker(offered: number[]): DeliveryWorker {
    return { enqueue: (row: { id: number }) => { offered.push(row.id); return Promise.resolve(); } } as unknown as DeliveryWorker;
  }

  test('a busy refusal is offered again; an uncertain receipt is marked, never resent', async () => {
    const busy = store.sendNudge('silas', 'kade', 'a');
    const unsure = store.sendNudge('silas', 'kade', 'b');
    store.markAgentQueued(busy, RUN, 'agent-queued');
    store.markAgentQueued(unsure, RUN, 'transport-accepted');
    const supervisor: AgentSupervisor = {
      send: () => Promise.resolve('queued' as const),
      receipts: () => Promise.resolve({ [`pulse:${unsure}`]: 'uncertain' }),
    };
    const offered: number[] = [];
    await reconcileAgentInbox(store, supervisor, worker(offered), noop);
    expect(offered).toEqual([busy]);
    const row = store.getAgentQueued().find((r) => r.id === unsure);
    expect(row?.last_delivery_error).toBe('uncertain');
  });

  test('negative proof: a row the hook already claimed is left alone, and a supervisor that cannot answer settles nothing', async () => {
    const claimed = store.sendNudge('silas', 'kade', 'a');
    const waiting = store.sendNudge('silas', 'kade', 'b');
    store.markAgentQueued(waiting, RUN, 'transport-accepted');
    store.claimAgentInbox('kade', RUN);
    const supervisor: AgentSupervisor = {
      send: () => Promise.resolve('queued' as const),
      receipts: () => Promise.reject(new Error('socket gone')),
    };
    const offered: number[] = [];
    await reconcileAgentInbox(store, supervisor, worker(offered), noop);
    expect(offered).toEqual([]);
    expect(store.getDeliveryRecord(claimed).delivery_status).toBe('queued');
    expect(store.getDeliveryRecord(waiting).delivery_status).toBe('queued');
  });
});
