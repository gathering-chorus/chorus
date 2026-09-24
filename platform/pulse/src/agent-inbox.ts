import type { Express } from 'express';
import type { AgentSupervisor } from './agent-supervisor';
import { resolvePulseSecret, secretsMatch } from './pulse-secret';
import { MessageStore } from './store';
import type { DeliveryWorker, EmitSpine } from './delivery-worker';

async function flushContextEvents(store: MessageStore, emit: EmitSpine): Promise<void> {
  for (const row of store.pendingContextEvents()) {
    await emit(row.kind === 'jeff-input' ? 'jeff.input.surfaced' : 'nudge.surfaced', {
      id: row.id, from: row.from, to: row.to, target: `agent:${row.delivery_session_id}`,
      status: 'context_delivered', ...(row.trace_id ? { trace_id: row.trace_id } : {}),
    });
    store.markContextEventEmitted(row.id);
  }
}

export function registerAgentInbox(app: Express, store: MessageStore, supervisor: AgentSupervisor, emit: EmitSpine = async () => {}): void {
  for (const action of ['claim', 'ack'] as const) app.post(`/api/agent-inbox/${action}`, async (req, res) => {
    const expected = resolvePulseSecret();
    const header = req.headers['x-chorus-pulse-secret'];
    // This door never inherits the historical nudge fail-open/test bypass.
    if (!expected || !secretsMatch(typeof header === 'string' ? header : undefined, expected)) { res.status(403).json({ error: 'unauthorized' }); return; }
    const { role, session_id: sessionId } = req.body ?? {};
    if (!['wren', 'silas', 'kade'].includes(role) || typeof sessionId !== 'string' || !/^[A-Za-z0-9_-]{1,200}$/.test(sessionId)) {
      res.status(400).json({ error: 'invalid-recipient' }); return;
    }
    try {
      const sessions = await supervisor.sessions();
      const target = sessions.find((s) => s.session_id === sessionId && s.role === role);
      if (!target || target.mode !== 'native' || ['disconnected', 'stopped', 'failed'].includes(target.state)) {
        res.status(409).json({ error: 'session-not-available' }); return;
      }
      // Multiple primaries are refused, never resolved by newest timestamp.
      if (target.primary && sessions.filter((s) => s.role === role && s.primary && !['stopped', 'failed'].includes(s.state)).length !== 1) {
        res.status(409).json({ error: 'ambiguous-primary' }); return;
      }
      if (action === 'claim') {
        const limit = req.body.limit ?? 50;
        if (!Number.isInteger(limit) || limit < 1 || limit > 100) { res.status(400).json({ error: 'invalid-limit' }); return; }
        res.json({ ok: true, messages: store.claimAgentInbox(role, sessionId, target.primary, limit) });
      } else {
        const ids: unknown = req.body.ids;
        if (!Array.isArray(ids) || ids.length > 100 || !ids.every((id) => Number.isSafeInteger(id) && id > 0) || new Set(ids).size !== ids.length) {
          res.status(400).json({ error: 'invalid-message-ids' }); return;
        }
        const acknowledged = store.acknowledgeAgentInbox(role, sessionId, ids, target.primary);
        // Delivery bookkeeping is durable; an audit sink outage cannot undo it.
        try { await flushContextEvents(store, emit); } catch { /* durable event outbox retries on reconciliation */ }
        res.json({ ok: true, acknowledged, status: 'context_delivered' });
      }
    } catch (err) {
      const mismatch = err instanceof Error && err.message === 'inbox-claim-mismatch';
      res.status(mismatch ? 409 : 503).json({ error: mismatch ? 'inbox-claim-mismatch' : 'supervisor-unavailable' });
    }
  });
}

/** Poll receipts, not message content. An uncertain send is never replayed.
 * Only messages the supervisor explicitly queued as busy may be retried. */
export async function reconcileAgentInbox(store: MessageStore, supervisor: AgentSupervisor, worker: DeliveryWorker, emit: EmitSpine): Promise<void> {
  await flushContextEvents(store, emit);
  const rows = store.getAgentQueued();
  if (!rows.length) return;
  const sessions = await supervisor.sessions();
  const receipts = new Map<string, Record<string, string>>();
  for (const row of rows) {
    if (row.last_delivery_error === 'native-boundary') continue;
    if (row.last_delivery_error === 'agent-busy' || row.last_delivery_error === 'supervisor-unavailable') {
      const matches = sessions.filter((s) => s.role === row.to && (row.delivery_session_id || row.target_session_id ? s.session_id === (row.delivery_session_id ?? row.target_session_id) : s.primary));
      if (matches.length === 1 && matches[0].state === 'idle') await worker.enqueue(row);
      continue;
    }
    const session = row.delivery_session_id;
    if (!session) continue;
    if (!receipts.has(session)) {
      try { receipts.set(session, await supervisor.receipts(session)); }
      catch { receipts.set(session, {}); }
    }
    const status = receipts.get(session)?.[`pulse:${row.id}`];
    if (status === 'context_delivered') {
      await emit(row.kind === 'jeff-input' ? 'jeff.input.surfaced' : 'nudge.surfaced', { id: row.id, from: row.from, to: row.to, target: `agent:${session}`, status, trace_id: row.trace_id });
      store.markDelivered(row.id);
    } else if (status === 'uncertain' && row.last_delivery_error !== 'uncertain') {
      store.markAgentQueued(row.id, session, 'uncertain');
    }
  }
}
