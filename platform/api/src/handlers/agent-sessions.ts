import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import type { Express, Request, Response } from 'express';
import { verifyAgentIdentity } from './agent-identity';

type VerifyDeps = Parameters<typeof verifyAgentIdentity>[1];
type AgentRecord = { session_id: string; principal: string; role: string; [key: string]: unknown };
export type AgentTransport = (method: string, route: string, body?: unknown) => Promise<Record<string, unknown>>;

/** Only the operator-owned socket is configurable; request data cannot select a host. */
export const agentTransport: AgentTransport = (method, route, body) => new Promise((resolve, reject) => {
  const payload = body === undefined ? undefined : JSON.stringify(body);
  const req = http.request({ socketPath: process.env.CHORUS_AGENT_SOCKET || path.join(os.homedir(), '.chorus/run/chorus-agent.sock'),
    path: route, method, headers: { 'Content-Type': 'application/json', ...(payload ? { 'Content-Length': Buffer.byteLength(payload) } : {}) } }, res => {
    let output = '';
    res.setEncoding('utf8');
    res.on('data', chunk => { output += chunk; if (Buffer.byteLength(output) > 4 * 1024 * 1024) req.destroy(new Error('agent response exceeds limit')); });
    res.on('error', reject);
    res.on('end', () => {
      try {
        const value = JSON.parse(output);
        if (!res.statusCode || res.statusCode >= 400) return reject(new Error('Agent supervisor refused the request'));
        resolve(value);
      } catch { reject(new Error('Invalid agent supervisor response')); }
    });
  });
  req.setTimeout(10000, () => req.destroy(new Error('Agent supervisor timed out')));
  req.on('error', reject);
  req.end(payload);
});

export function canControlAgent(identity: { principal: string; role: string }, session: AgentRecord): boolean {
  return identity.role === 'jeff' || (identity.principal === session.principal && identity.role === session.role);
}

export function mountAgentSessions(app: Express, deps: VerifyDeps, transport: AgentTransport = agentTransport): void {
  const authorize = async (req: Request, res: Response) => {
    const decision = await verifyAgentIdentity(req.headers.authorization || '', deps);
    if (decision.status !== 200 || !('principal' in decision.body)) { res.status(decision.status).json(decision.body); return null; }
    return decision.body as { principal: string; role: string; scopes: string[] };
  };
  app.get('/api/chorus/agent-sessions', async (req, res) => {
    const identity = await authorize(req, res); if (!identity) return;
    try {
      const data = await transport('GET', '/v1/sessions');
      const sessions = (data.sessions as AgentRecord[]).filter(s => canControlAgent(identity, s));
      res.setHeader('Cache-Control', 'no-store'); res.json({ version: 1, sessions });
    } catch { res.status(503).json({ error: 'agent_supervisor_unavailable' }); }
  });
  for (const action of ['status', 'events', 'send', 'cancel', 'stop'] as const) {
    const handler = async (req: Request, res: Response) => {
      const identity = await authorize(req, res); if (!identity) return;
      const id = String(req.params.id);
      if (!/^[A-Za-z0-9_-]{1,200}$/.test(id)) { res.status(400).json({ error: 'invalid_session_id' }); return; }
      try {
        const session = await transport('GET', `/v1/sessions/${id}`) as AgentRecord;
        if (!canControlAgent(identity, session)) { res.status(403).json({ error: 'session_not_authorized' }); return; }
        if (action === 'send' && req.body?.kind === 'human_input' && identity.role !== 'jeff') {
          res.status(403).json({ error: 'human_input_requires_human_identity' }); return;
        }
        let envelope: Record<string, unknown> | undefined;
        if (action === 'send') {
          const { version, message_id, input, kind = 'peer_message' } = req.body ?? {};
          if (version !== 1 || typeof message_id !== 'string' || !/^[A-Za-z0-9_.:-]{1,200}$/.test(message_id)
            || typeof input !== 'string' || input.length === 0 || Buffer.byteLength(input) > 4 * 1024 * 1024
            || !['peer_message', 'human_input'].includes(kind)) {
            res.status(400).json({ error: 'invalid_input_envelope' }); return;
          }
          // The supervisor deliberately does not persist native message bodies.
          // Refuse this path before admission; Pulse owns the durable inbox.
          if (session.mode === 'native') {
            res.status(409).json({ error: 'native_requires_pulse_inbox', persisted: false,
              message: 'Use chorus_nudge_message with target_session_id to queue native session context in Pulse.' }); return;
          }
          envelope = { version, message_id, input, kind };
        }
        let route = `/v1/sessions/${id}${action === 'status' ? '' : `/${action}`}`;
        if (action === 'events') {
          const after = String(req.query.after ?? '0');
          if (!/^\d{1,16}$/.test(after)) { res.status(400).json({ error: 'invalid_cursor' }); return; }
          route += `?after=${after}`;
        }
        const output = action === 'status' ? session : await transport(action === 'events' ? 'GET' : 'POST', route, envelope);
        res.setHeader('Cache-Control', 'no-store'); res.json(output);
      } catch { res.status(503).json({ error: 'agent_operation_unavailable' }); }
    };
    const route = `/api/chorus/agent-sessions/:id${action === 'status' ? '' : `/${action}`}`;
    if (action === 'status' || action === 'events') app.get(route, handler); else app.post(route, handler);
  }
}
