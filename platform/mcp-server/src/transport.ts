/**
 * #2472 / #2949 — Streamable HTTP transport mount for the MCP server.
 *
 * Mounts at POST /mcp on the chorus-api Express app. Sender role read from
 * X-Chorus-Role header (falls back to CHORUS_ROLE env).
 *
 * Stateless mode (#2949). The transport is constructed without a
 * sessionIdGenerator, so the SDK's validateSession returns undefined
 * immediately on every request — no init handshake required, no session-id
 * matching, no "Server not initialized" failure mode after chorus-api
 * kickstart. Each request creates a fresh transport (the SDK explicitly
 * requires this for stateless mode: "Reusing a stateless transport causes
 * message ID collisions between clients").
 *
 * Earlier attempts at this problem (#2937 transparent-reinit + #2946
 * adopt-client-sessionId) introduced a synthesize-init scaffolding that
 * compiled but never actually flipped the SDK's _initialized flag because
 * the discard-response stub didn't satisfy Hono's adapter contract. Both
 * paths and their per-session sessions Map have been removed here in
 * favor of statelessness, which is simpler and proven offline against
 * /tmp/show-it-works.sh (init/list → process exits → restart → same list
 * call succeeds, no reconnect).
 */
import type { Application, Request, Response } from 'express';
import { StreamableHTTPServerTransport } from '@modelcontextprotocol/sdk/server/streamableHttp.js';
import { buildMcpServer, executeNudge, type FetchImpl, type NudgeArgs } from './server';
import { execFile } from 'child_process';
import { promisify } from 'util';
import { randomUUID } from 'crypto';
import { authenticateAgentRequest, apiIdentityVerifier, apiSessionVerifier, identityMode, AgentAuthError, type AgentIdentity, type AgentAuthDeps } from './request-identity';

// #3000 — transport-level error capture. Emit typed mcp.transport.error
// spine events on non-2xx /mcp responses + connection-level failures.
// Closes the "behind MCP boundary errors vaporize" gap at the transport
// layer (per-tool errors are captured inside server.ts's dispatch wrap).
//
// #3001 — also push notify to silas via pulse so ops sees errors in real
// time. POST is fire-and-forget; pulse failure logs but doesn't cascade.
const execFileAsync = promisify(execFile);

// #3429 — safe stringify for unknown field values (no [object Object] from
// template/String coercion; satisfies @typescript-eslint/no-base-to-string).
function str(v: unknown): string {
  if (v === null || v === undefined) return '';
  if (typeof v === 'string') return v;
  if (typeof v === 'number' || typeof v === 'boolean') return String(v);
  return JSON.stringify(v);
}

// #3958 — bounded, never-throws rendering of a request body for the
// transport-error event. 300 chars is enough to name the method/shape.
export function safeBodySnippet(body: unknown): string {
  try {
    if (body === undefined || body === null) return '';
    const s = typeof body === 'string' ? body : JSON.stringify(body);
    return (s ?? '').slice(0, 300);
  } catch {
    return '[unserializable]';
  }
}

// #3963 — the benign claude-code discovery probe. Claude Code's CLI POSTs a
// `server/discover` (a method our MCP SDK does not implement), the SDK returns
// 400, and #3001's push model then nudged silas indefinitely (16+ times/day)
// for a request that is NOT an ops fault. Suppress the NUDGE for exactly this
// signature — the spine event is still emitted, so observability is preserved.
// Signature-scoped, never a blanket mute: a real error 400 still nudges.
export function isBenignDiscoveryProbe(fields: Record<string, unknown>): boolean {
  const status = str(fields['status'] ?? '');
  if (status !== '400') return false;
  const ua = str(fields['user_agent'] ?? '').toLowerCase();
  if (!ua.includes('claude-code')) return false;
  const body = str(fields['body'] ?? '');
  return body.includes('server/discover');
}

async function emitTransportError(fields: Record<string, unknown>): Promise<void> {
  try {
    const args = ['mcp.transport.error', str(fields['from'] ?? 'unknown')];
    for (const [k, v] of Object.entries(fields)) {
      if (k === 'from') continue;
      args.push(`${k}=${typeof v === 'string' ? v : JSON.stringify(v)}`);
    }
    await execFileAsync('chorus-log', args, { timeout: 2000 });
  } catch {
    // best-effort; chorus-log failure must not affect the HTTP response
  }
  // #3001 — push notify to silas in parallel with spine emit.
  // #3963 — but NOT for the benign claude-code discovery probe (spine kept).
  if (!isBenignDiscoveryProbe(fields)) {
    void notifyTransportError(fields);
  }
}

async function notifyTransportError(fields: Record<string, unknown>): Promise<void> {
  const summary = [
    '[mcp.error] mcp.transport.error',
    fields['method'] && `${str(fields['method'])} ${str(fields['path'])}`,
    fields['status'] && `status=${str(fields['status'])}`,
    fields['kind'] && `kind=${str(fields['kind'])}`,
    fields['error_message'] && `msg=${str(fields['error_message']).slice(0, 200)}`,
  ].filter(Boolean).join(' ');
  // #3485 — route through the single execution path (executeNudge), not a
  // direct pulse POST. In-package call; best-effort (errors must not affect
  // the HTTP response). executeNudge is the only thing that POSTs pulse, and
  // it owns the pulse URL (no URL named here).
  const fetchAdapter: FetchImpl = (url, init) =>
    fetch(url, init as RequestInit) as unknown as ReturnType<FetchImpl>;
  try {
    await executeNudge({ to: 'silas', message: summary } as NudgeArgs, 'chorus-mcp', fetchAdapter);
  } catch (err) {

    console.error('[chorus-mcp] mcp.notification.failed', { reason: err instanceof Error ? err.message : String(err) });
  }
}

export function mountMcpEndpoint(app: Application, auth?: AgentAuthDeps): void {
  const deps = auth ?? {
    mode: identityMode(process.env.CHORUS_MCP_IDENTITY_MODE),
    verify: apiIdentityVerifier(process.env.CHORUS_API_URL || 'http://localhost:3340'),
    session: apiSessionVerifier(process.env.CHORUS_API_URL || 'http://localhost:3340'),
    legacyRole: process.env.CHORUS_ROLE,
  };
  async function authenticate(req: Request, res: Response): Promise<AgentIdentity | null> {
    try {
      return await authenticateAgentRequest({
        authorization: req.header('Authorization'),
        sessionId: req.header('X-Chorus-Session-Id'),
        role: req.header('X-Chorus-Role'),
      }, deps);
    } catch (err) {
      const failure = err instanceof AgentAuthError ? err : new AgentAuthError(503, 'identity-unavailable');
      res.status(failure.status).json({ jsonrpc: '2.0', id: req.body?.id ?? null, error: { code: -32001, message: failure.reason } });
      return null;
    }
  }
  app.post('/mcp', async (req: Request, res: Response) => {
    const identity = await authenticate(req, res);
    if (!identity) return;
    const callerRole = identity.role;
    // #3008 — emit Mcp-Session-Id response header per MCP HTTP+SSE spec so
    // spec-conformant clients (chorus-hooks mcp_client.rs:65-68 requires it
    // on initialize and errors "no session id header" when absent) get the
    // handshake they expect. The UUID is purely informational: server stays
    // stateless because StreamableHTTPServerTransport is constructed
    // without sessionIdGenerator (#2949 invariant), so the SDK never stores
    // or validates session-ids. Header set before SDK takes over the
    // response so it persists through transport.handleRequest.
    res.setHeader('Mcp-Session-Id', randomUUID());
    // #3000 — capture transport-level errors. Listen for response 'finish'
    // (non-2xx) and connection 'close'/'error' (mid-stream client drop).
    res.on('finish', () => {
      if (res.statusCode < 200 || res.statusCode >= 300) {
        // Fire-and-forget; emitTransportError is best-effort.
        // #3958 — a bare status=400 event is undiagnosable: 16 nudges in one
        // day and no way to name the caller. Carry user-agent + a body
        // snippet so one live occurrence identifies the client and request.
        void emitTransportError({
          from: callerRole,
          method: 'POST',
          path: '/mcp',
          status: res.statusCode,
          user_agent: str(req.headers['user-agent'] ?? ''),
          body: safeBodySnippet(req.body),
        });
      }
    });
    req.on('aborted', () => {
      void emitTransportError({
        from: callerRole,
        method: 'POST',
        path: '/mcp',
        kind: 'client-aborted',
      });
    });
    try {
      const transport = new StreamableHTTPServerTransport({});
      const server = buildMcpServer(() => callerRole, { identity });
      await server.connect(transport);
      await transport.handleRequest(req, res, req.body);
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : String(err);
      void emitTransportError({
        from: callerRole,
        method: 'POST',
        path: '/mcp',
        kind: 'handler-throw',
        error_message: errorMessage.slice(0, 500),
      });
      if (!res.headersSent) {
        res.status(500).json({ jsonrpc: '2.0', error: { code: -32603, message: errorMessage } });
      }
    }
  });

  // GET /mcp is used by the SDK for SSE notification streams. In stateless
  // mode there's no persistent server-side state to stream from, so we
  // accept the connection and let the SDK handle it on a per-request
  // transport (same shape as POST). For Chorus's request/response tool-call
  // workload, no client today depends on SSE notifications.
  app.get('/mcp', async (req: Request, res: Response) => {
    const identity = await authenticate(req, res);
    if (!identity) return;
    const callerRole = identity.role;
    // #3008 — same header treatment as POST. GET /mcp opens an SSE
    // notification stream; spec-conformant clients expect the session-id
    // here too.
    res.setHeader('Mcp-Session-Id', randomUUID());
    const transport = new StreamableHTTPServerTransport({});
    const server = buildMcpServer(() => callerRole, { identity });
    await server.connect(transport);
    await transport.handleRequest(req, res);
  });

  app.delete('/mcp', async (req: Request, res: Response) => {
    if (!(await authenticate(req, res))) return;
    // Stateless: no session state to delete. 204 No Content preserves the
    // contract clients expect.
    res.status(204).end();
  });
}
