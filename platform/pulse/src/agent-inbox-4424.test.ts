// @domain: messages
// @test-type: unit — express app over a temp messages.db, Presence from fixtures; no live services
// @card: #4424
// @owner: wren
/**
 * #4424 — a native runtime's hook claims its run's messages at a safe boundary
 * and acknowledges them once they are in context. Who may claim is decided by
 * Presence/SessionRun (the role's live agent run), never by a supervisor list.
 */
import * as fs from 'fs';
import * as path from 'path';
import express from 'express';
import request from 'supertest';
import { registerAgentInbox, reconcileAgentInbox } from './agent-inbox';
import { resolveFromPresence } from './presence-target';
import { MessageStore } from './store';
import { DeliveryWorker, type RunInject } from './delivery-worker';
import type { AgentSupervisor } from './agent-supervisor';

const SECRET = 'test-secret-4424';
const KADE_RUN = 'kade-run-live';
const CLAIM = '/api/agent-inbox/claim';
const SECRET_HEADER = 'x-chorus-pulse-secret';
const noop = (): Promise<void> => Promise.resolve();
const runs = [{ name: KADE_RUN, runEndedAt: '' }, { name: 'kade-run-old', runEndedAt: '2026-10-04T08:00:00Z' }];
const presences = [
  { name: 'kade-presence-now', presenceOf: 'session-run-kade-run-live', reachableOver: 'agent' },
  { name: 'kade-presence-old', presenceOf: 'session-run-kade-run-old', reachableOver: 'agent' },
];
const resolve = (role: string) => Promise.resolve(resolveFromPresence(presences, runs, role));

const DB = path.join(__dirname, '..', 'test-agent-inbox-4424.db');
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

function bareApp() {
  const app = express();
  app.use(express.json());
  return app;
}

describe('agent inbox claim/ack is decided by the live run (#4424 step 4)', () => {
  test('the live agent run claims, then acks into delivered', async () => {
    const events: string[] = [];
    const app = bareApp();
    registerAgentInbox(app, store, resolve, (event) => { events.push(event); return Promise.resolve(); });
    const id = store.sendNudge('silas', 'kade', 'hello');
    const claim = await request(app).post(CLAIM).set(SECRET_HEADER, SECRET)
      .send({ role: 'kade', session_id: KADE_RUN });
    expect(claim.status).toBe(200);
    expect(claim.body.messages.map((m: { id: number }) => m.id)).toEqual([id]);
    const ack = await request(app).post('/api/agent-inbox/ack').set(SECRET_HEADER, SECRET)
      .send({ role: 'kade', session_id: KADE_RUN, ids: [id] });
    expect(ack.status).toBe(200);
    expect(store.getDeliveryRecord(id).delivery_status).toBe('delivered');
    expect(events).toContain('nudge.surfaced');
  });

  test('negative proof: an ended run cannot claim, even with the secret', async () => {
    const app = bareApp();
    registerAgentInbox(app, store, resolve);
    const id = store.sendNudge('silas', 'kade', 'hello');
    const claim = await request(app).post(CLAIM).set(SECRET_HEADER, SECRET)
      .send({ role: 'kade', session_id: 'kade-run-old' });
    expect(claim.status).toBe(409);
    expect(store.getDeliveryRecord(id).delivery_status).toBe('pending');
  });

  test('negative proof: another role cannot claim this role run, and no secret is 403', async () => {
    const app = bareApp();
    registerAgentInbox(app, store, resolve);
    store.sendNudge('silas', 'kade', 'hello');
    const wrongRole = await request(app).post(CLAIM).set(SECRET_HEADER, SECRET)
      .send({ role: 'wren', session_id: KADE_RUN });
    expect(wrongRole.status).toBe(409);
    const noSecret = await request(app).post(CLAIM).send({ role: 'kade', session_id: KADE_RUN });
    expect(noSecret.status).toBe(403);
  });
});

describe('receipt reconciliation copies the supervisor receipt into the message row', () => {
  test('context_delivered settles delivered; transport_accepted stays queued; nothing is resent', async () => {
    const delivered = store.sendNudge('silas', 'kade', 'a');
    const accepted = store.sendNudge('silas', 'kade', 'b');
    store.markAgentQueued(delivered, KADE_RUN, 'transport-accepted');
    store.markAgentQueued(accepted, KADE_RUN, 'transport-accepted');
    let sends = 0;
    const supervisor: AgentSupervisor = {
      send() { sends++; return Promise.resolve('queued' as const); },
      receipts() { return Promise.resolve({ [`pulse:${delivered}`]: 'context_delivered', [`pulse:${accepted}`]: 'transport_accepted' }); },
    };
    const runInject: RunInject = () => { sends++; return Promise.resolve({ rc: 0, stderr: '' }); };
    const worker = new DeliveryWorker(store, runInject, noop, [1], noop);
    await reconcileAgentInbox(store, supervisor, worker, noop);
    expect(store.getDeliveryRecord(delivered).delivery_status).toBe('delivered');
    expect(store.getDeliveryRecord(accepted).delivery_status).toBe('queued');
    expect(sends).toBe(0);
  });
});
