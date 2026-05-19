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

// #3000 — transport-level error capture. Emit typed mcp.transport.error
// spine events on non-2xx /mcp responses + connection-level failures.
// Closes the "behind MCP boundary errors vaporize" gap at the transport
// layer (per-tool errors are captured inside server.ts's dispatch wrap).
//
// #3001 — also push notify to silas via pulse so ops sees errors in real
// time. POST is fire-and-forget; pulse failure logs but doesn't cascade.
const execFileAsync = promisify(execFile);
async function emitTransportError(fields: Record<string, unknown>): Promise<void> {
  try {
    const args = ['mcp.transport.error', String(fields['from'] ?? 'unknown')];
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
    fields['method'] && `${fields['method']} ${fields['path']}`,
    fields['status'] && `status=${fields['status']}`,
    fields['kind'] && `kind=${fields['kind']}`,
    fields['error_message'] && `msg=${String(fields['error_message']).slice(0, 200)}`,
  ].filter(Boolean).join(' ');
  const pulseUrl = process.env.CHORUS_PULSE_URL || 'http://localhost:3475/api/nudge';
  try {
    const ctrl = new AbortController();
    const timeoutId = setTimeout(() => ctrl.abort(), 2000);
    const resp = await fetch(pulseUrl, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'X-Chorus-MCP-Caller': '1' },
      body: JSON.stringify({ from: 'chorus-mcp', to: 'silas', content: summary }),
      signal: ctrl.signal,
    });
    clearTimeout(timeoutId);
    if (!resp.ok) {
      // log via stderr (chorus-log spawn already attempted above)
      // eslint-disable-next-line no-console
      console.error('[chorus-mcp] mcp.notification.failed', { reason: `pulse-${resp.status}` });
    }
  } catch {
    // best-effort
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
