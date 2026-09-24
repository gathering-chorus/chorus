import { request } from 'node:http';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';
import type { InjectResult, RunInject } from './delivery-worker';

export interface AgentSession {
  session_id: string; role: string; primary: boolean;
  mode: 'native' | 'managed'; state: string;
}
export type AgentReceipt = 'queued' | 'transport_accepted' | 'context_delivered' | 'uncertain';
export interface AgentSupervisor {
  sessions(): Promise<AgentSession[]>;
  send(session: string, messageId: string, input: string, kind: 'peer_message' | 'human_input'): Promise<AgentReceipt>;
  receipts(session: string): Promise<Record<string, string>>;
}

function validSession(value: unknown): value is AgentSession {
  const s = value as Partial<AgentSession> | null;
  return !!s && typeof s.session_id === 'string' && /^[A-Za-z0-9_-]{1,200}$/.test(s.session_id)
    && typeof s.role === 'string' && typeof s.primary === 'boolean'
    && (s.mode === 'native' || s.mode === 'managed') && ['idle', 'running', 'awaiting_approval', 'disconnected', 'failed', 'stopped'].includes(s.state ?? '');
}

/** Local supervisor metadata only. Pulse is still the sole message-body owner. */
export class LocalAgentSupervisor implements AgentSupervisor {
  constructor(
    private root = process.env.CHORUS_AGENT_STATE_DIR || join(homedir(), '.chorus'),
    private socket = process.env.CHORUS_AGENT_SOCKET || join(root, 'run/chorus-agent.sock'),
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

  async sessions(): Promise<AgentSession[]> {
    if (!existsSync(this.socket)) {
      // A missing daemon must not send an enrolled role through a legacy TTY.
      // Durable v2 registrations are used ONLY to block that fallback, never to
      // claim live delivery. A genuinely unenrolled installation returns [].
      const dir = join(this.root, 'sessions/v2');
      if (!existsSync(dir)) return [];
      return readdirSync(dir).filter((p) => p.endsWith('.json')).map((p) => {
        const s: unknown = JSON.parse(readFileSync(join(dir, p), 'utf8'));
        if (!validSession(s)) throw new Error('invalid-agent-registry');
        return { session_id: s.session_id, role: s.role, primary: s.primary, mode: s.mode, state: s.state === 'stopped' || s.state === 'failed' ? s.state : 'disconnected' };
      });
    }
    const result = await this.call('GET', '/v1/sessions') as { version?: number; sessions?: unknown[] };
    if (result.version !== 1 || !Array.isArray(result.sessions) || !result.sessions.every(validSession)) throw new Error('invalid-agent-registry');
    return result.sessions;
  }
  async send(session: string, messageId: string, input: string, kind: 'peer_message' | 'human_input'): Promise<AgentReceipt> {
    const result = await this.call('POST', `/v1/sessions/${encodeURIComponent(session)}/send`, { version: 1, message_id: messageId, input, kind }) as { status?: string };
    if (!['queued', 'transport_accepted', 'context_delivered', 'uncertain'].includes(result.status ?? '')) throw new Error('invalid-agent-receipt');
    return result.status as AgentReceipt;
  }
  async receipts(session: string): Promise<Record<string, string>> {
    const result = await this.call('GET', `/v1/sessions/${encodeURIComponent(session)}/receipts`) as { receipts?: Record<string, string> };
    if (!result.receipts || typeof result.receipts !== 'object') throw new Error('invalid-agent-receipts');
    return result.receipts;
  }
}

export function preferAgentSupervisor(legacy: RunInject, supervisor: AgentSupervisor): RunInject {
  return async (to, content, from, messageId, options) => {
    let sessions: AgentSession[];
    try { sessions = await supervisor.sessions(); }
    catch { return { rc: 0, stderr: '', deferred: true, deferReason: 'supervisor-unavailable', agentSessionId: options?.targetSessionId }; }
    const candidates = options?.targetSessionId
      ? sessions.filter((s) => s.session_id === options.targetSessionId && s.role === to)
      : sessions.filter((s) => s.role === to && s.primary);
    if (!candidates.length && !options?.targetSessionId) return legacy(to, content, from, messageId, options);
    if (candidates.length !== 1) return { rc: 0, stderr: '', deferred: true, deferReason: 'undelivered-agent-target-unresolved' };
    const session = candidates[0];
    const target = `agent:${session.session_id}`;
    const base: InjectResult = { rc: 0, stderr: '', target, agentSessionId: session.session_id };
    if (!messageId) return { ...base, deferred: true, deferReason: 'undelivered-message-id-required' };
    if (session.state === 'disconnected' || session.state === 'stopped' || session.state === 'failed') {
      return { ...base, deferred: true, deferReason: 'supervisor-unavailable' };
    }
    try {
      const receipt = await supervisor.send(session.session_id, messageId, content, options?.kind === 'jeff-input' ? 'human_input' : 'peer_message');
      if (receipt === 'context_delivered') return { ...base, agentReceipt: receipt };
      return { ...base, deferred: true, agentReceipt: receipt, deferReason: receipt === 'queued' ? (session.mode === 'native' ? 'native-boundary' : 'agent-busy') : receipt.replace('_', '-') };
    } catch {
      // An interrupted send may already have been admitted. Never blind retry.
      return { ...base, deferred: true, deferReason: 'uncertain', agentReceipt: 'uncertain' };
    }
  };
}
