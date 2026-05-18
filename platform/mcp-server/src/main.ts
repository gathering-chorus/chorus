// chorus-mcp main entry. Separate process from chorus-api so chorus-api
// redeploys do not kill role-session MCP transports (#2997).
//
// Listens on CHORUS_MCP_PORT (default 3341). chorus-api still mounts /mcp
// at 3340 during this card — cutover is #2998.

import express, { Application, Request, Response } from 'express';
import { mountMcpEndpoint } from './transport';

const PORT = parseInt(process.env.CHORUS_MCP_PORT || '3341', 10);

const app: Application = express();
app.use(express.json({ limit: '10mb' }));

// Health endpoint — mirrors chorus-api shape so chorus-deploy MCP-smoke and
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

app.listen(PORT, () => {
  // eslint-disable-next-line no-console
  console.log(`[chorus-mcp] Listening on 0.0.0.0:${PORT}`);
});

process.on('uncaughtException', (err) => {
  // eslint-disable-next-line no-console
  console.error('[chorus-mcp] FATAL uncaughtException:', err.message);
  console.error(err);
  process.exit(1);
});
