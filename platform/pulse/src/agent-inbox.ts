// @domain: messages
/**
 * #4424 — the agent inbox: a native runtime's hook claims its run's messages at
 * a safe boundary (session start, prompt) and acknowledges them once they are
 * written into context. Who may claim is the role's live agent run, read from
 * Presence/SessionRun — the one answer to "where is this role" (#4361).
 *
 * The reconcile loop copies the supervisor's receipt for each admitted message
 * into its messages.db row; messages.db stays the record.
 */
import type { Express } from 'express';
import type { AgentSupervisor } from './agent-supervisor';
import { resolvePulseSecret, secretsMatch } from './pulse-secret';
import { MessageStore } from './store';
import type { DeliveryWorker, EmitSpine } from './delivery-worker';
import type { TypedResolution } from './session-registry';
import { fetchPresenceResolution } from './presence-target';

type Resolve = (role: string) => Promise<TypedResolution | { kind: 'unread'; why: string }>;

async function flushContextEvents(store: MessageStore, emit: EmitSpine): Promise<void> {
  for (const row of store.pendingContextEvents()) {
    await emit(row.kind === 'jeff-input' ? 'jeff.input.surfaced' : 'nudge.surfaced', {
      id: row.id, from: row.from, to: row.to, target: `agent:${row.delivery_session_id}`,
      status: 'context_delivered', ...(row.trace_id ? { trace_id: row.trace_id } : {}),
    });
    store.markContextEventEmitted(row.id);
  }
}

type Req = { headers: Record<string, unknown>; body?: Record<string, unknown> };
type Res = { status(code: number): { json(body: unknown): void } };

/** The secret, the recipient shape, and the live run — in that order. Returns
 * the role and run when the caller may use this door, else answers itself. */
async function admit(req: Req, res: Res, resolve: Resolve): Promise<{ role: string; run: string } | null> {
  const expected = resolvePulseSecret();
  const header = req.headers['x-chorus-pulse-secret'];
  // This door never inherits the historical nudge fail-open/test bypass.
  if (!expected || !secretsMatch(typeof header === 'string' ? header : undefined, expected)) { res.status(403).json({ error: 'unauthorized' }); return null; }
  const role = req.body?.role;
  const run = req.body?.session_id;
  if (typeof role !== 'string' || !['wren', 'silas', 'kade'].includes(role) || typeof run !== 'string' || !/^[A-Za-z0-9_-]{1,200}$/.test(run)) {
    res.status(400).json({ error: 'invalid-recipient' }); return null;
  }
  const where = await resolve(role);
  if (where.kind === 'unread') { res.status(503).json({ error: 'presence-unread', why: where.why }); return null; }
  // Only the role's own live agent run may claim; an ended run never can.
  if (where.kind !== 'agent' || where.run !== run) { res.status(409).json({ error: 'session-not-the-live-run' }); return null; }
  return { role, run };
}

function validIds(ids: unknown): ids is number[] {
  return Array.isArray(ids) && ids.length <= 100 && ids.every((id) => Number.isSafeInteger(id) && id > 0) && new Set(ids).size === ids.length;
}

export function registerAgentInbox(app: Express, store: MessageStore, resolve: Resolve = fetchPresenceResolution, emit: EmitSpine = () => Promise.resolve()): void {
  app.post('/api/agent-inbox/claim', async (req, res) => {
    const who = await admit(req, res, resolve);
    if (!who) return;
    const limit = req.body.limit ?? 50;
    if (!Number.isInteger(limit) || limit < 1 || limit > 100) { res.status(400).json({ error: 'invalid-limit' }); return; }
    res.json({ ok: true, messages: store.claimAgentInbox(who.role, who.run, limit) });
  });
  app.post('/api/agent-inbox/ack', async (req, res) => {
    const who = await admit(req, res, resolve);
    if (!who) return;
    if (!validIds(req.body.ids)) { res.status(400).json({ error: 'invalid-message-ids' }); return; }
    let acknowledged: number;
    try { acknowledged = store.acknowledgeAgentInbox(who.role, who.run, req.body.ids); }
    catch { res.status(409).json({ error: 'inbox-claim-mismatch' }); return; }
    // Delivery bookkeeping is durable; an audit sink outage cannot undo it.
    try { await flushContextEvents(store, emit); } catch { /* the reconcile loop retries the outbox */ }
    res.json({ ok: true, acknowledged, status: 'context_delivered' });
  });
}

type QueuedRow = ReturnType<MessageStore['getAgentQueued']>[number];

/** Copy one admitted row's supervisor receipt into its messages.db row. */
async function settleFromReceipt(row: QueuedRow, run: string, receipt: string | undefined, store: MessageStore, emit: EmitSpine): Promise<void> {
  if (receipt === 'context_delivered') {
    await emit(row.kind === 'jeff-input' ? 'jeff.input.surfaced' : 'nudge.surfaced', { id: row.id, from: row.from, to: row.to, target: `agent:${run}`, status: receipt, trace_id: row.trace_id });
    store.markDelivered(row.id);
  } else if (receipt === 'uncertain' && row.last_delivery_error !== 'uncertain') {
    store.markAgentQueued(row.id, run, 'uncertain');
  }
}

/** Poll receipts, not message content. An admitted or uncertain send is never
 * replayed; only a row the supervisor refused as busy is offered again. */
export async function reconcileAgentInbox(store: MessageStore, supervisor: AgentSupervisor, worker: DeliveryWorker, emit: EmitSpine): Promise<void> {
  await flushContextEvents(store, emit);
  const receipts = new Map<string, Record<string, string>>();
  const receiptsOf = async (run: string): Promise<Record<string, string>> => {
    if (!receipts.has(run)) receipts.set(run, await supervisor.receipts(run).catch(() => ({})));
    return receipts.get(run) ?? {};
  };
  for (const row of store.getAgentQueued()) {
    const run = row.delivery_session_id;
    if (!run || row.last_delivery_error === 'native-boundary') continue;
    if (row.last_delivery_error === 'agent-queued' || row.last_delivery_error === 'supervisor-unavailable') {
      await worker.enqueue(row);
      continue;
    }
    await settleFromReceipt(row, run, (await receiptsOf(run))[`pulse:${row.id}`], store, emit);
  }
}
