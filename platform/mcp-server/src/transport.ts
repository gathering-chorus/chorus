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
    const transport = new StreamableHTTPServerTransport({});
    const server = buildMcpServer(() => callerRole);
    await server.connect(transport);
    await transport.handleRequest(req, res, req.body);
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
