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
import { buildMcpServer } from './server';
import { execFile } from 'child_process';
import { promisify } from 'util';
import { randomUUID } from 'crypto';
import { executeNudge, type FetchImpl, type NudgeArgs } from './server';

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
  // #3001 — push notify to silas in parallel with spine emit
  void notifyTransportError(fields);
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

function resolveCallerRole(req: Request): string {
  const headerRole = req.header('X-Chorus-Role');
  if (headerRole && /^(silas|wren|kade|jeff)$/.test(headerRole)) {
    return headerRole;
  }
  const envRole = process.env.CHORUS_ROLE;
  if (envRole && /^(silas|wren|kade|jeff)$/.test(envRole)) {
    return envRole;
  }
  return 'unknown';
}

export function mountMcpEndpoint(app: Application): void {
  app.post('/mcp', async (req: Request, res: Response) => {
    const callerRole = resolveCallerRole(req);
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
        void emitTransportError({
          from: callerRole,
          method: 'POST',
          path: '/mcp',
          status: res.statusCode,
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
      const server = buildMcpServer(() => callerRole);
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
    const callerRole = resolveCallerRole(req);
    // #3008 — same header treatment as POST. GET /mcp opens an SSE
    // notification stream; spec-conformant clients expect the session-id
    // here too.
    res.setHeader('Mcp-Session-Id', randomUUID());
    const transport = new StreamableHTTPServerTransport({});
    const server = buildMcpServer(() => callerRole);
    await server.connect(transport);
    await transport.handleRequest(req, res);
  });

  // eslint-disable-next-line @typescript-eslint/require-await -- Express handler signature, no async work needed
  app.delete('/mcp', async (_req: Request, res: Response) => {
    // Stateless: no session state to delete. 204 No Content preserves the
    // contract clients expect.
    res.status(204).end();
  });
}
