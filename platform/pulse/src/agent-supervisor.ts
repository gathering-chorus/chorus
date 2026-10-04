// @domain: messages
/**
 * #4424 — delivery to a role running on another model, through the local agent
 * supervisor (chorus-agentd).
 *
 * Pulse never asks the supervisor where a role is. That answer has one home:
 * the role's Presence row and its live SessionRun (#4361). When that Presence
 * is reachableOver agent, the supervisor is the transport and the run's name is
 * its session id. Pulse stays the sole owner of message bodies (messages.db).
 */
import { request } from 'node:http';
import { homedir } from 'node:os';
import { join } from 'node:path';
import type { InjectResult, RunInject } from './delivery-worker';
import type { TypedResolution } from './session-registry';
import { fetchPresenceResolution } from './presence-target';

export type AgentReceipt = 'queued' | 'transport_accepted' | 'context_delivered' | 'uncertain';
export interface AgentSupervisor {
  send(run: string, messageId: string, input: string, kind: 'peer_message' | 'human_input'): Promise<AgentReceipt>;
  receipts(run: string): Promise<Record<string, string>>;
}

/** HTTP over the supervisor's Unix socket. Human input carries no token: the
 * supervisor accepts it because Pulse runs as Jeff's account (its human_uids). */
export class LocalAgentSupervisor implements AgentSupervisor {
  constructor(
    private socket = process.env.CHORUS_AGENT_SOCKET
      || join(process.env.CHORUS_AGENT_STATE_DIR || join(homedir(), '.chorus'), 'run/chorus-agent.sock'),
  ) {}

  private call(method: string, path: string, body?: unknown): Promise<unknown> {
    return new Promise((resolve, reject) => {
      const req = request({ socketPath: this.socket, method, path, headers: { 'Content-Type': 'application/json' } }, (response) => {
        let text = '';
        response.setEncoding('utf8');
        response.on('data', (chunk: string) => {
          text += chunk;
          if (text.length > 1024 * 1024) req.destroy(new Error('supervisor-response-too-large'));
        });
        response.on('error', reject);
        response.on('end', () => {
          if (!response.statusCode || response.statusCode < 200 || response.statusCode >= 300) { reject(new Error('supervisor-refused')); return; }
          try { resolve(JSON.parse(text)); } catch { reject(new Error('supervisor-response-invalid')); }
        });
      });
      req.setTimeout(5000, () => req.destroy(new Error('supervisor-timeout')));
      req.on('error', reject);
      req.end(body === undefined ? undefined : JSON.stringify(body));
    });
  }

  async send(run: string, messageId: string, input: string, kind: 'peer_message' | 'human_input'): Promise<AgentReceipt> {
    const result = await this.call('POST', `/v1/sessions/${encodeURIComponent(run)}/send`, { version: 1, message_id: messageId, input, kind }) as { status?: string };
    if (!['queued', 'transport_accepted', 'context_delivered', 'uncertain'].includes(result.status ?? '')) throw new Error('invalid-agent-receipt');
    return result.status as AgentReceipt;
  }

  async receipts(run: string): Promise<Record<string, string>> {
    const result = await this.call('GET', `/v1/sessions/${encodeURIComponent(run)}/receipts`) as { receipts?: Record<string, string> };
    if (!result.receipts || typeof result.receipts !== 'object') throw new Error('invalid-agent-receipts');
    return result.receipts;
  }
}

type Resolve = (role: string) => Promise<TypedResolution | { kind: 'unread'; why: string }>;

/**
 * Route by the role's Presence. kind 'agent' → the supervisor, addressed by the
 * live run; every other kind → the legacy (tmux) path, unchanged. An explicit
 * target is honored only when it is that role's live agent run; it never falls
 * back to another session or to a pane.
 */
export function routeByPresence(legacy: RunInject, supervisor: AgentSupervisor, resolve: Resolve = fetchPresenceResolution): RunInject {
  return async (to, content, from, messageId, options) => {
    const res = await resolve(to);
    const explicit = options?.targetSessionId;
    // A role live on both a pane and an agent run has no single answer: refuse,
    // typed and visible, rather than let either transport win silently.
    if (res.kind === 'ambiguous') return { rc: 0, stderr: '', deferred: true, deferReason: 'undelivered-ambiguous', target: `undelivered:${to}:ambiguous` };
    if (res.kind !== 'agent') {
      if (explicit) return { rc: 0, stderr: '', deferred: true, deferReason: 'undelivered-agent-target-not-live', target: `undelivered:${to}:${explicit}` };
      return legacy(to, content, from, messageId, options);
    }
    if (explicit && explicit !== res.run) {
      return { rc: 0, stderr: '', deferred: true, deferReason: 'undelivered-agent-target-not-live', target: `undelivered:${to}:${explicit}` };
    }
    const base: InjectResult = { rc: 0, stderr: '', target: `agent:${res.run}`, agentSessionId: res.run };
    if (!messageId) return { ...base, deferred: true, deferReason: 'undelivered-message-id-required' };
    try {
      const receipt = await supervisor.send(res.run, messageId, content, options?.kind === 'jeff-input' ? 'human_input' : 'peer_message');
      if (receipt === 'context_delivered') return { ...base, agentReceipt: receipt };
      // Admission is not delivery: everything short of context_delivered stays
      // queued, bound to this run.
      return { ...base, deferred: true, agentReceipt: receipt, deferReason: receipt === 'queued' ? 'agent-queued' : receipt.replace('_', '-') };
    } catch {
      // An interrupted send may already have been admitted. Never blind retry.
      return { ...base, deferred: true, deferReason: 'uncertain', agentReceipt: 'uncertain' };
    }
  };
}
