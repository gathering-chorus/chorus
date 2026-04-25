/**
 * #2472 — Streamable HTTP transport mount for the MCP server.
 *
 * Mounts at POST /mcp on the chorus-api Express app. Per-request session map
 * keys on Mcp-Session-Id header; sender role read from X-Chorus-Role header
 * (falls back to CHORUS_ROLE env on the server side, which is fine for local
 * single-role-per-process scenarios but headers are the canonical input from
 * Claude Code).
 */
import type { Application, Request, Response } from 'express';
import { StreamableHTTPServerTransport } from '@modelcontextprotocol/sdk/server/streamableHttp.js';
import { randomUUID } from 'crypto';
import { buildMcpServer } from './server';

interface SessionEntry {
  transport: StreamableHTTPServerTransport;
  callerRole: string;
}

const sessions = new Map<string, SessionEntry>();

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
        sessionIdGenerator: () => randomUUID(),
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
