/**
 * #2472 — Streamable HTTP transport mount for the MCP server.
 *
 * Mounts at POST /mcp on the chorus-api Express app. Per-request session map
 * keys on Mcp-Session-Id header; sender role read from X-Chorus-Role header.
 *
 * #2937 — chorus-api kickstart used to invalidate every active client's MCP
 * session. The in-memory sessions Map empties on restart; client retries
 * with old session id hit "Server not initialized" from the freshly-created
 * transport (the new transport hasn't received an initialize handshake).
 * The transparent-reinit path below synthesizes an initialize against a
 * discard-response when stale-session is detected, before forwarding the
 * real request. Client never sees the failure; only the post-restart
 * request pays one extra synthetic round-trip.
 */
import type { Application, Request, Response } from 'express';
import { StreamableHTTPServerTransport } from '@modelcontextprotocol/sdk/server/streamableHttp.js';
import { randomUUID } from 'crypto';
import { EventEmitter } from 'events';
import { buildMcpServer } from './server';

interface SessionEntry {
  transport: StreamableHTTPServerTransport;
  callerRole: string;
}

const sessions = new Map<string, SessionEntry>();

/**
 * #2937 — discard-response for transparent reinit. The SDK transport's
 * handleRequest writes to a Response-shaped object; for the synthetic
 * initialize call we want a sink that absorbs the response without
 * touching the real client's res. EventEmitter provides on/once/emit;
 * the rest are minimal stubs.
 */
class DiscardResponse extends EventEmitter {
  statusCode = 200;
  headersSent = false;
  writeHead(_code: number, _headers?: Record<string, string>): this {
    this.headersSent = true;
    return this;
  }
  write(_chunk: unknown): boolean {
    return true;
  }
  end(_chunk?: unknown): this {
    this.emit('finish');
    return this;
  }
  setHeader(_name: string, _value: string | string[]): this {
    return this;
  }
  getHeader(_name: string): undefined {
    return undefined;
  }
  flushHeaders(): void {
    /* no-op */
  }
}

/**
 * #2937 — synthesize an initialize handshake against a fresh transport so
 * subsequent requests don't fail with "Server not initialized" after a
 * chorus-api restart. Best-effort; logs on failure but does not throw.
 */
async function synthesizeInitialize(
  transport: StreamableHTTPServerTransport,
  req: Request,
): Promise<void> {
  const initBody = {
    jsonrpc: '2.0',
    id: 0,
    method: 'initialize',
    params: {
      protocolVersion: '2025-11-25',
      capabilities: {},
      clientInfo: { name: 'chorus-api-transparent-reinit', version: '1.0.0' },
    },
  };
  const syntheticReq = {
    method: 'POST',
    url: req.url,
    headers: { ...req.headers, 'content-type': 'application/json' },
    header: (name: string) => req.header(name),
    body: initBody,
  } as unknown as Request;
  const sink = new DiscardResponse();
  try {
    await transport.handleRequest(syntheticReq, sink as unknown as Response, initBody);
    process.stderr.write(
      JSON.stringify({
        level: 'info',
        event: 'mcp.session.transparent-reinit',
        sessionId: req.header('Mcp-Session-Id') ?? '',
        ts: new Date().toISOString(),
      }) + '\n',
    );
  } catch (err) {
    process.stderr.write(
      JSON.stringify({
        level: 'warn',
        event: 'mcp.session.transparent-reinit.failed',
        sessionId: req.header('Mcp-Session-Id') ?? '',
        error: err instanceof Error ? err.message : String(err),
        ts: new Date().toISOString(),
      }) + '\n',
    );
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
    const sessionId = (req.header('Mcp-Session-Id') as string | undefined) ?? '';
    const callerRole = resolveCallerRole(req);

    let entry = sessions.get(sessionId);

    if (!entry) {
      const transport = new StreamableHTTPServerTransport({
        // #2946 — when client provided a session-id (resurrection path after
        // chorus-api restart), the fresh transport must adopt that id so the
        // synthesize-init binding matches the client's subsequent calls.
        // Without this, the transport mints a new UUID, transport.sessionId
        // ≠ client header, real request fails with "Server not initialized".
        sessionIdGenerator: () => sessionId || randomUUID(),
        onsessioninitialized: (newId: string) => {
          sessions.set(newId, { transport, callerRole });
          process.stderr.write(
            JSON.stringify({
              level: 'info',
              event: 'mcp.session.initialized',
              sessionId: newId,
              callerRole,
              ts: new Date().toISOString(),
            }) + '\n',
          );
        },
      });

      const server = buildMcpServer(() => {
        const current = transport.sessionId
          ? sessions.get(transport.sessionId)
          : undefined;
        return current?.callerRole ?? callerRole;
      });

      await server.connect(transport);

      transport.onclose = () => {
        if (transport.sessionId) {
          sessions.delete(transport.sessionId);
          process.stderr.write(
            JSON.stringify({
              level: 'info',
              event: 'mcp.session.closed',
              sessionId: transport.sessionId,
              ts: new Date().toISOString(),
            }) + '\n',
          );
        }
      };

      // #2937 — transparent reinit. If client has a non-empty sessionId but
      // we have no entry for it, the server restarted between requests.
      // Synthesize an initialize handshake against the fresh transport
      // before forwarding the real request, so the client never sees the
      // "Server not initialized" error and doesn't need to manually /mcp
      // reconnect. Only fires when the incoming body itself is NOT an
      // initialize — if it already is, the real request handles itself.
      const body = req.body as { method?: string } | undefined;
      const isInitialize = body?.method === 'initialize';
      if (sessionId && !isInitialize) {
        await synthesizeInitialize(transport, req);
      }

      entry = { transport, callerRole };
      if (sessionId) sessions.set(sessionId, entry);
    } else {
      entry.callerRole = callerRole;
    }

    await entry.transport.handleRequest(req, res, req.body);
  });

  app.get('/mcp', async (req: Request, res: Response) => {
    const sessionId = req.header('Mcp-Session-Id') as string | undefined;
    if (!sessionId) {
      res.status(400).json({ error: 'Mcp-Session-Id header required' });
      return;
    }
    const entry = sessions.get(sessionId);
    if (!entry) {
      res.status(404).json({ error: 'session not found' });
      return;
    }
    await entry.transport.handleRequest(req, res);
  });

  // eslint-disable-next-line @typescript-eslint/require-await -- Express handler signature, no async work needed
  app.delete('/mcp', async (req: Request, res: Response) => {
    const sessionId = req.header('Mcp-Session-Id') as string | undefined;
    if (sessionId) sessions.delete(sessionId);
    res.status(204).end();
  });
}
