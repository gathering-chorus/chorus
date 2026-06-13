// chorus-mcp main entry. Separate process from chorus-api so chorus-api
// redeploys do not kill role-session MCP transports (#2997).
//
// Listens on CHORUS_MCP_PORT (default 3341). chorus-api still mounts /mcp
// at 3340 during this card — cutover is #2998.

import express, { Application, Request, Response } from 'express';
import { mountMcpEndpoint } from './transport';
import { execFileSync } from 'child_process';

// #3000 — process-level error capture. Emit mcp.process.error to spine
// before exit so a crash is observable to ops, not silent. Uses sync exec
// because the process is on its way out.
function emitProcessError(kind: string, err: unknown): void {
  try {
    const msg = err instanceof Error ? err.message : String(err);
    execFileSync(
      'chorus-log',
      [
        'mcp.process.error',
        process.env.CHORUS_ROLE || 'chorus-mcp',
        `kind=${kind}`,
        `error_message=${msg.slice(0, 500)}`,
      ],
      { timeout: 2000, stdio: 'ignore' },
    );
  } catch {
    // best-effort
  }
}

const PORT = parseInt(process.env.CHORUS_MCP_PORT || '3341', 10);
// #3390 — internal service, no cross-machine consumer: bind localhost, not
// 0.0.0.0 (ADR-042 §8, ADR-012 intent restored). CHORUS_BIND override exists
// for the rare LAN case; default is loopback.
const BIND_HOST = process.env.CHORUS_BIND || '127.0.0.1';

const app: Application = express();
app.use(express.json({ limit: '10mb' }));

// Health endpoint — mirrors chorus-api shape so the werk-deploy MCP-smoke and
// chorus-health probes can hit a known URL.
const START_TS = Date.now();
app.get('/api/chorus/health', (_req: Request, res: Response) => {
  res.json({
    status: 'healthy',
    service: 'chorus-mcp',
    uptime: Math.floor((Date.now() - START_TS) / 1000),
    timestamp: new Date().toISOString().slice(0, 19).replace('T', ' '),
  });
});

// Mount the MCP endpoint at /mcp — same path shape as chorus-api so client
// configs only need to change host:port, not the path.
mountMcpEndpoint(app);

app.listen(PORT, BIND_HOST, () => {
  // eslint-disable-next-line no-console
  console.log(`[chorus-mcp] Listening on ${BIND_HOST}:${PORT}`);
});

process.on('uncaughtException', (err) => {
  // #3000 — emit to spine before logging + exit so the crash is observable
  // to chorus-health rather than silent.
  emitProcessError('uncaughtException', err);
  // eslint-disable-next-line no-console
  console.error('[chorus-mcp] FATAL uncaughtException:', err.message);
  console.error(err);
  process.exit(1);
});

process.on('unhandledRejection', (reason) => {
  emitProcessError('unhandledRejection', reason);
  // eslint-disable-next-line no-console
  console.error('[chorus-mcp] FATAL unhandledRejection:', reason);
  process.exit(1);
});
