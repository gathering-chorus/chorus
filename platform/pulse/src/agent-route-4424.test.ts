// @domain: messages
// @test-type: unit — Presence fixtures, a fake supervisor, a temp messages.db; no live services
// @card: #4424
// @owner: wren
/**
 * #4424 — Pulse delivers to a role running on another model through the agent
 * supervisor, chosen by the role's Presence (reachableOver agent), never by a
 * session list of the supervisor's own. Each test is a negative proof (#3734).
 */
import * as fs from 'fs';
import * as path from 'path';
import { resolveFromPresence } from './presence-target';
import { routeByPresence, type AgentSupervisor, type AgentReceipt } from './agent-supervisor';
import { DeliveryWorker, type InjectResult, type RunInject } from './delivery-worker';
import { MessageStore } from './store';
import type { TypedResolution } from './session-registry';

const KADE_RUN = 'kade-run-live';
const runs = [
  { name: KADE_RUN, runEndedAt: '' },
  { name: 'wren-run-live', runEndedAt: '' },
  { name: 'silas-run-ended', runEndedAt: '2026-10-04T09:00:00Z' },
];
const presences = [
  { name: 'kade-presence-1', presenceOf: 'session-run-kade-run-live', reachableOver: 'agent' },
  { name: 'wren-presence-1', presenceOf: 'session-run-wren-run-live', pane: '%7', tty: '/dev/ttys006', reachableOver: 'nudge' },
  { name: 'silas-presence-1', presenceOf: 'session-run-silas-run-ended', reachableOver: 'agent' },
];

function fakeSupervisor(receipt: AgentReceipt = 'context_delivered') {
  const sent: Array<{ run: string; messageId: string; kind: string }> = [];
  const supervisor: AgentSupervisor = {
    send(run, messageId, _input, kind) { sent.push({ run, messageId, kind }); return Promise.resolve(receipt); },
    receipts() { return Promise.resolve({}); },
  };
  return { supervisor, sent };
}
function fakeLegacy() {
  const calls: string[] = [];
  const legacy: RunInject = (to) => { calls.push(to); return Promise.resolve({ rc: 0, stderr: '', target: `tmux:${to}` }); };
  return { legacy, calls };
}
const resolverFor = (role: string) => (): Promise<TypedResolution> => Promise.resolve(resolveFromPresence(presences, runs, role));
const noop = (): Promise<void> => Promise.resolve();
const SOURCE_LIST_CALL = /\.sessions\(\)|sessions\/v2/;

describe('(a) reachableOver agent goes to the supervisor; a nudge Presence still goes to tmux', () => {
  test('an agent Presence on a live run resolves to that run', () => {
    expect(resolveFromPresence(presences, runs, 'kade')).toEqual({ kind: 'agent', run: KADE_RUN });
  });

  test('agent: supervisor.send gets the run name; legacy is not called', async () => {
    const { supervisor, sent } = fakeSupervisor();
    const { legacy, calls } = fakeLegacy();
    const r = await routeByPresence(legacy, supervisor, resolverFor('kade'))('kade', 'hi', 'silas', 'pulse:1');
    expect(sent).toEqual([{ run: KADE_RUN, messageId: 'pulse:1', kind: 'peer_message' }]);
    expect(calls).toEqual([]);
    expect(r.deferred).toBeFalsy();
    expect(r.agentSessionId).toBe(KADE_RUN);
  });

  test('nudge Presence: legacy (tmux) is called; the supervisor is not', async () => {
    const { supervisor, sent } = fakeSupervisor();
    const { legacy, calls } = fakeLegacy();
    const r = await routeByPresence(legacy, supervisor, resolverFor('wren'))('wren', 'hi', 'silas', 'pulse:2');
    expect(calls).toEqual(['wren']);
    expect(sent).toEqual([]);
    expect(r.target).toBe('tmux:wren');
    expect(resolveFromPresence(presences, runs, 'wren').kind).toBe('resolved');
  });

  test('human input to an agent run is sent as human_input (no token forwarded)', async () => {
    const { supervisor, sent } = fakeSupervisor();
    const { legacy } = fakeLegacy();
    await routeByPresence(legacy, supervisor, resolverFor('kade'))('kade', 'from jeff', 'jeff', 'pulse:3', { kind: 'jeff-input' });
    expect(sent[0].kind).toBe('human_input');
  });
});

describe('(b) a dead run is never sent to', () => {
  test('an agent Presence whose run has runEndedAt resolves dead, not agent', () => {
    expect(resolveFromPresence(presences, runs, 'silas').kind).toBe('dead');
  });

  test('an explicit target that is not the role live run is refused without a send', async () => {
    const { supervisor, sent } = fakeSupervisor();
    const { legacy, calls } = fakeLegacy();
    const route = routeByPresence(legacy, supervisor, resolverFor('silas'));
    const r = await route('silas', 'hi', 'kade', 'pulse:4', { targetSessionId: 'silas-run-ended' });
    expect(sent).toEqual([]);
    expect(calls).toEqual([]);
    expect(r.deferred).toBe(true);
    expect(r.deferReason).toMatch(/^undelivered-/);
  });

  test('target_session_id other than the live agent run is refused, the live run is accepted', async () => {
    const { supervisor, sent } = fakeSupervisor();
    const { legacy } = fakeLegacy();
    const route = routeByPresence(legacy, supervisor, resolverFor('kade'));
    const bad = await route('kade', 'hi', 'silas', 'pulse:5', { targetSessionId: 'kade-run-other' });
    expect(bad.deferReason).toMatch(/^undelivered-/);
    expect(sent).toEqual([]);
    await route('kade', 'hi', 'silas', 'pulse:6', { targetSessionId: KADE_RUN });
    expect(sent.map((s) => s.run)).toEqual([KADE_RUN]);
  });
});

describe('(c) transport_accepted keeps the row queued, never delivered', () => {
  const DB = path.join(__dirname, '..', 'test-agent-route-4424.db');
  let store: MessageStore;
  beforeEach(() => { if (fs.existsSync(DB)) fs.unlinkSync(DB); store = new MessageStore(DB); });
  afterEach(() => { try { store.close(); } catch { /* closed */ } if (fs.existsSync(DB)) fs.unlinkSync(DB); });

  test('admitted but not in context: queued, bound to the run, not resent', async () => {
    const id = store.sendNudge('silas', 'kade', 'hello');
    let sends = 0;
    const accepted: RunInject = (): Promise<InjectResult> => {
      sends++;
      return Promise.resolve({ rc: 0, stderr: '', deferred: true, deferReason: 'transport-accepted', agentSessionId: KADE_RUN, agentReceipt: 'transport_accepted' });
    };
    const worker = new DeliveryWorker(store, accepted, noop, [1], noop);
    await worker.enqueue({ id, from: 'silas', to: 'kade', content: 'hello', delivery_attempts: 0 });
    let rec = store.getDeliveryRecord(id);
    expect(rec.delivery_status).toBe('queued');
    expect(rec.last_delivery_error).toBe('transport-accepted');
    expect(rec.delivery_session_id).toBe(KADE_RUN);
    // A second pass over the same row (restart scan) must not send it again.
    await worker.enqueue({ id, from: 'silas', to: 'kade', content: 'hello', delivery_attempts: 1 });
    expect(sends).toBe(1);
    rec = store.getDeliveryRecord(id);
    expect(rec.delivery_status).toBe('queued');
  });

  test('context_delivered is the only success', async () => {
    const id = store.sendNudge('silas', 'kade', 'hello');
    const delivered: RunInject = () => Promise.resolve({ rc: 0, stderr: '', target: `agent:${KADE_RUN}`, agentSessionId: KADE_RUN, agentReceipt: 'context_delivered' as const });
    const worker = new DeliveryWorker(store, delivered, noop, [1], noop);
    await worker.enqueue({ id, from: 'silas', to: 'kade', content: 'hello', delivery_attempts: 0 });
    expect(store.getDeliveryRecord(id).delivery_status).toBe('delivered');
  });

  test('an agent-busy row is not released by the legacy /drain, so a rescan sends nothing', async () => {
    const id = store.sendNudge('silas', 'kade', 'hello');
    store.markAgentQueued(id, KADE_RUN, 'agent-busy');
    expect(store.drainQueued('kade')).toBe(0);
    let sends = 0;
    const counting: RunInject = () => { sends++; return Promise.resolve({ rc: 0, stderr: '' }); };
    const worker = new DeliveryWorker(store, counting, noop, [1], noop);
    await worker.scanAndRequeue();
    expect(sends).toBe(0);
    expect(store.getDeliveryRecord(id).delivery_status).toBe('queued');
  });
});

describe('(d) pulse never asks the supervisor for a session list', () => {
  test('routing touches only supervisor.send, and no pulse source lists sessions', async () => {
    // Runtime proof: a supervisor that records every property read.
    const touched: string[] = [];
    const base = fakeSupervisor().supervisor;
    const spy = new Proxy(base, { get(target, prop, recv) { touched.push(String(prop)); return Reflect.get(target, prop, recv); } });
    const { legacy } = fakeLegacy();
    await routeByPresence(legacy, spy, resolverFor('kade'))('kade', 'hi', 'silas', 'pulse:7');
    await routeByPresence(legacy, spy, resolverFor('wren'))('wren', 'hi', 'silas', 'pulse:8');
    expect(touched).toEqual(['send']);
    // Source proof: no file in pulse calls .sessions() or reads sessions/v2.
    const dir = __dirname;
    const offenders = fs.readdirSync(dir)
      .filter((f) => f.endsWith('.ts') && !f.endsWith('.test.ts'))
      // eslint-disable-next-line security/detect-non-literal-fs-filename -- reads this package's own src dir, listed just above; never caller input.
      .filter((f) => SOURCE_LIST_CALL.exec(fs.readFileSync(path.join(dir, f), 'utf8')) !== null);
    expect(offenders).toEqual([]);
    // The guard must be able to fail: it reads the real source dir, and the
    // pattern does match the PR's own call.
    expect(fs.readdirSync(dir)).toContain('agent-supervisor.ts');
    expect(SOURCE_LIST_CALL.exec('await supervisor.sessions()')).not.toBeNull();
  });
});
