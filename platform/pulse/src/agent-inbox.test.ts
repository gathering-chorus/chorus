import request from 'supertest';
import { createApp } from './service';
import { MessageStore } from './store';
import { DeliveryWorker } from './delivery-worker';
import { AgentSession, AgentSupervisor, LocalAgentSupervisor, preferAgentSupervisor } from './agent-supervisor';
import { reconcileAgentInbox } from './agent-inbox';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const primary: AgentSession = { session_id: 'native-1', role: 'wren', primary: true, mode: 'native', state: 'idle' };
let store: MessageStore;
let supervisor: jest.Mocked<AgentSupervisor>;
let emit: jest.Mock;
let legacy: jest.Mock;
let worker: DeliveryWorker;
beforeEach(() => {
  process.env.CHORUS_PULSE_SECRET = 'test-inbox-secret';
  store = new MessageStore(':memory:');
  supervisor = {
    sessions: jest.fn<ReturnType<AgentSupervisor['sessions']>, Parameters<AgentSupervisor['sessions']>>().mockResolvedValue([primary]),
    send: jest.fn<ReturnType<AgentSupervisor['send']>, Parameters<AgentSupervisor['send']>>().mockResolvedValue('queued'),
    receipts: jest.fn<ReturnType<AgentSupervisor['receipts']>, Parameters<AgentSupervisor['receipts']>>().mockResolvedValue({}),
  };
  emit = jest.fn(async () => {});
  legacy = jest.fn(async () => ({ rc: 0, stderr: '' }));
  worker = new DeliveryWorker(store, preferAgentSupervisor(legacy, supervisor), emit);
});
afterEach(() => { store.close(); delete process.env.CHORUS_PULSE_SECRET; });
function post(action: 'claim' | 'ack', body: Record<string, unknown>, secret = 'test-inbox-secret') {
  return request(createApp(store, worker, supervisor, emit)).post(`/api/agent-inbox/${action}`).set('X-Chorus-Pulse-Secret', secret).send({ role: 'wren', session_id: 'native-1', ...body });
}

test('native admission stays queued; repeatable claim and idempotent ack record context, never peer reply', async () => {
  const id = store.sendNudge('silas', 'wren', 'native message', 'trace');
  await worker.enqueue(store.getPendingDeliveries()[0]);
  expect(supervisor.send).toHaveBeenCalledWith('native-1', `pulse:${id}`, 'native message', 'peer_message');
  expect(store.getDeliveryRecord(id)).toMatchObject({ delivery_status: 'queued', delivered_at: null, last_delivery_error: 'native-boundary' });
  expect(legacy).not.toHaveBeenCalled();
  expect(store.drainQueued('wren')).toBe(0);
  const first = await post('claim', {});
  expect(first.status).toBe(200);
  expect(first.body.messages).toEqual([{ id, message_id: `pulse:${id}`, from: 'silas', content: 'native message', kind: 'peer_message' }]);
  expect((await post('claim', {})).body).toEqual(first.body);
  expect(store.getDeliveryRecord(id).delivery_status).toBe('queued');
  expect((await post('ack', { ids: [id] })).body).toEqual({ ok: true, acknowledged: 1, status: 'context_delivered' });
  expect((await post('ack', { ids: [id] })).body.acknowledged).toBe(0);
  expect((await post('claim', {})).body.messages).toEqual([]);
  expect(emit.mock.calls.filter(([event]) => event === 'nudge.surfaced')).toHaveLength(1);
  expect(emit.mock.calls.some(([event]) => String(event).includes('replied'))).toBe(false);
});

test('secret, live role binding, secondary targeting, and atomic ack ownership are enforced', async () => {
  const roleId = store.sendNudge('silas', 'wren', 'role message');
  const targeted = store.sendNudge('silas', 'wren', 'secondary message', undefined, 'r2r', 'none', 'secondary');
  const secondary = { ...primary, session_id: 'secondary', primary: false };
  supervisor.sessions.mockResolvedValue([primary, secondary]);
  expect((await post('claim', {}, 'wrong')).status).toBe(403);
  expect((await post('claim', { role: 'silas' })).status).toBe(409);
  expect((await post('claim', { session_id: 'secondary' })).body.messages.map((m: { id: number }) => m.id)).toEqual([targeted]);
  expect((await post('ack', { session_id: 'secondary', ids: [targeted, roleId] })).status).toBe(409);
  expect(store.getDeliveryRecord(targeted).delivery_status).toBe('queued');
  expect((await post('claim', {})).body.messages.map((m: { id: number }) => m.id)).toEqual([roleId]);
  expect((await post('ack', { ids: [targeted] })).status).toBe(409);
  expect((await post('ack', { session_id: 'secondary', ids: [targeted] })).status).toBe(200);
  supervisor.sessions.mockResolvedValue([{ ...primary, state: 'disconnected' }]);
  expect((await post('ack', { ids: [roleId] })).status).toBe(409);
});

test('managed transport admission and uncertain outcome never count as delivered or replay', async () => {
  supervisor.sessions.mockResolvedValue([{ ...primary, mode: 'managed' }]);
  supervisor.send.mockResolvedValue('transport_accepted');
  const id = store.sendJeffInput('wren', 'human message');
  const stalePending = store.getPendingDeliveries()[0];
  await worker.enqueue(stalePending);
  await worker.enqueue(stalePending);
  expect(store.getDeliveryRecord(id).delivery_status).toBe('queued');
  expect(supervisor.send).toHaveBeenCalledWith('native-1', `pulse:${id}`, 'human message', 'human_input');
  supervisor.receipts.mockResolvedValue({ [`pulse:${id}`]: 'uncertain' });
  await reconcileAgentInbox(store, supervisor, worker, emit);
  expect(store.getDeliveryRecord(id).last_delivery_error).toBe('uncertain');
  await reconcileAgentInbox(store, supervisor, worker, emit);
  expect(supervisor.send).toHaveBeenCalledTimes(1);
  expect(store.drainQueued('wren')).toBe(0);
  supervisor.receipts.mockResolvedValue({ [`pulse:${id}`]: 'context_delivered' });
  await reconcileAgentInbox(store, supervisor, worker, emit);
  expect(store.getDeliveryRecord(id).delivery_status).toBe('delivered');
  expect(emit.mock.calls.filter(([event]) => event === 'jeff.input.surfaced')).toHaveLength(1);
});

test('busy managed session retries its bound session; explicit unresolved and ambiguous targets never fall back', async () => {
  supervisor.sessions.mockResolvedValue([{ ...primary, mode: 'managed', state: 'running' }]);
  const id = store.sendNudge('silas', 'wren', 'busy message');
  await worker.enqueue(store.getPendingDeliveries()[0]);
  await reconcileAgentInbox(store, supervisor, worker, emit);
  expect(supervisor.send).toHaveBeenCalledTimes(1);
  supervisor.sessions.mockResolvedValue([{ ...primary, mode: 'managed' }]);
  supervisor.send.mockResolvedValue('context_delivered');
  await reconcileAgentInbox(store, supervisor, worker, emit);
  expect(store.getDeliveryRecord(id).delivery_status).toBe('delivered');
  const send = preferAgentSupervisor(legacy, supervisor);
  expect((await send('wren', 'message', 'silas', 'pulse:2', { targetSessionId: 'missing' })).deferReason).toBe('undelivered-agent-target-unresolved');
  supervisor.sessions.mockResolvedValue([primary, { ...primary, session_id: 'other' }]);
  expect((await send('wren', 'message', 'silas', 'pulse:3')).deferReason).toBe('undelivered-agent-target-unresolved');
  expect(legacy).not.toHaveBeenCalled();
});

test('native surfaced event survives sink failure and replays without resending body', async () => {
  const id = store.sendNudge('silas', 'wren', 'event retry');
  await post('claim', {});
  emit.mockRejectedValueOnce(new Error('sink offline'));
  expect((await post('ack', { ids: [id] })).body.status).toBe('context_delivered');
  expect(store.pendingContextEvents()).toHaveLength(1);
  await reconcileAgentInbox(store, supervisor, worker, emit);
  expect(store.pendingContextEvents()).toHaveLength(0);
  expect(supervisor.send).not.toHaveBeenCalled();
});

test('missing supervisor uses persisted registration only to block legacy fallback and never exposes credentials', async () => {
  const root = mkdtempSync(join(tmpdir(), 'pulse-agent-'));
  try {
    mkdirSync(join(root, 'sessions/v2'), { recursive: true });
    writeFileSync(join(root, 'sessions/v2/native-1.json'), JSON.stringify({ ...primary, credential_file: '/private/token' }));
    const local = new LocalAgentSupervisor(root, join(root, 'absent.sock'));
    expect(await local.sessions()).toEqual([{ ...primary, state: 'disconnected' }]);
    expect((await preferAgentSupervisor(legacy, local)('wren', 'message', 'silas', 'pulse:1')).deferReason).toBe('supervisor-unavailable');
    expect(legacy).not.toHaveBeenCalled();
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('claimed messages and delivery-event outbox survive database reopen', () => {
  const root = mkdtempSync(join(tmpdir(), 'pulse-inbox-restart-'));
  let disk: MessageStore | undefined;
  try {
    const db = join(root, 'messages.db');
    disk = new MessageStore(db);
    const id = disk.sendJeffInput('wren', 'survive restart', undefined, 'native-1');
    expect(disk.claimAgentInbox('wren', 'native-1', true)).toHaveLength(1);
    disk.close(); disk = new MessageStore(db);
    expect(disk.claimAgentInbox('wren', 'native-1', true)[0].id).toBe(id);
    expect(disk.claimAgentInbox('wren', 'different', true)).toEqual([]);
    expect(disk.acknowledgeAgentInbox('wren', 'native-1', [id])).toBe(1);
    disk.close(); disk = new MessageStore(db);
    expect(disk.getDeliveryRecord(id).delivery_status).toBe('delivered');
    expect(disk.pendingContextEvents()[0].id).toBe(id);
    expect(disk.acknowledgeAgentInbox('wren', 'native-1', [id])).toBe(0);
  } finally { disk?.close(); rmSync(root, { recursive: true, force: true }); }
});

test('outbound source attribution requires real shared secret and live matching session; sibling replies remain distinct', async () => {
  supervisor.sessions.mockResolvedValue([primary, { ...primary, session_id: 'secondary', primary: false }]);
  const app = createApp(store, undefined, supervisor);
  const body = { from: 'wren', to: 'kade', content: 'same peer reply', source_session_id: 'native-1' };
  process.env.PULSE_ALLOW_DIRECT_POST = '1';
  try {
    expect((await request(app).post('/api/nudge').send(body)).status).toBe(403);
    expect((await request(app).post('/api/nudge').set('X-Chorus-Pulse-Secret', 'test-inbox-secret').send({ ...body, from: 'silas' })).status).toBe(403);
    expect(store.getPendingDeliveries()).toEqual([]);
    expect((await request(app).post('/api/nudge').set('X-Chorus-Pulse-Secret', 'test-inbox-secret').send(body)).status).toBe(200);
    expect((await request(app).post('/api/nudge').set('X-Chorus-Pulse-Secret', 'test-inbox-secret').send({ ...body, source_session_id: 'secondary' })).status).toBe(200);
    expect(store.getPendingDeliveries().map(r => r.source_session_id)).toEqual(['native-1', 'secondary']);
    supervisor.sessions.mockResolvedValue([{ ...primary, state: 'stopped' }]);
    expect((await request(app).post('/api/nudge').set('X-Chorus-Pulse-Secret', 'test-inbox-secret').send({ ...body, content: 'after stop' })).status).toBe(403);
  } finally { delete process.env.PULSE_ALLOW_DIRECT_POST; }
});
