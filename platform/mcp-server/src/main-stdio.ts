// #3020 — stdio-per-session entry for chorus-mcp.
//
// Runs chorus-mcp as a process spawned per session over stdio, instead of a
// shared HTTP daemon on :3341. Reuses the SAME buildMcpServer the HTTP
// transport uses (transport.ts) — only the transport differs. Sound because
// the server is stateless by design (#2949: the HTTP path builds a fresh
// server per request, no sessionIdGenerator, no sessions map), so one server
// per spawn holds nothing the daemon held across requests.
//
// This is the "server" leg of #3020's "no per-type exception": a server
// becomes uniform with binaries/scripts in the WERK_ROLE_BIN model once it
// spawns from PATH like anything else — no per-role port, no .mcp.json
// repoint, no /reboot.
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { execFileSync } from 'child_process';
import { resolve } from 'path';
import { buildMcpServer } from './server';

// #3020 — operational visibility. A stdio session has no daemon and no port, so
// without this it leaves no trace on the spine — invisible to ops (the gap that
// made "do you log this?" a no). Best-effort, fire-and-forget: a logging failure
// must never affect the MCP server. Mirrors main.ts's emitProcessError.
//
// chorus-log is resolved by ABSOLUTE path relative to this module, not a bare
// PATH lookup: a stdio server spawned per session does NOT inherit the daemon's
// PATH, so `chorus-log` would silently no-op (verified — the emit evaporated).
// dist/ and src/ are both one level under mcp-server, so ../../scripts works for
// both `node dist/main-stdio.js` and `tsx src/main-stdio.ts`. Uses __dirname
// (project compiles to CommonJS; import.meta is rejected by tsc here).
const CHORUS_LOG = resolve(__dirname, '..', '..', 'scripts', 'chorus-log');
function emitSpine(event: string, fields: string[]): void {
  try {
    execFileSync(CHORUS_LOG, [event, process.env.CHORUS_ROLE || 'chorus-mcp', ...fields], {
      timeout: 2000,
      stdio: 'ignore',
    });
  } catch {
    // best-effort — logging must not affect the server
  }
}

/**
 * Build the chorus-mcp server configured for a stdio session. Same server the
 * HTTP transport builds; factored out so it can be exercised under test
 * without a real stdio pipe (the end-to-end spawn is the /demo).
 */
const VALID_ROLES = /^(silas|wren|kade)$/;

export function buildStdioServer(
  role?: string,
  deps?: Parameters<typeof buildMcpServer>[1],
): ReturnType<typeof buildMcpServer> {
  // Fail loud, never default (Kade gate flag): the server hosts commit/acp/nudge,
  // which attribute by role. A wrong/absent role would silently misattribute.
  const resolved = role ?? process.env.CHORUS_ROLE;
  if (!resolved || !VALID_ROLES.test(resolved)) {
    throw new Error(
      `[chorus-mcp-stdio] CHORUS_ROLE must be one of silas|wren|kade; got ${JSON.stringify(resolved)}. ` +
        'Refusing to default — a wrong role silently misattributes commit/acp/nudge (#3020).',
    );
  }
  return buildMcpServer(() => resolved, deps);
}

async function main(): Promise<void> {
  const server = buildStdioServer();
  const transport = new StdioServerTransport();

  // operational visibility: TEARDOWN. Every started event needs a matching
  // stopped event, or the spine shows sessions being born and never dying —
  // you can't tell a clean exit from a crash, or measure how long a session
  // lived. A stdio session ends when the client closes the pipe (stdin EOF) or
  // the process is signalled. The SDK transport does NOT surface stdin-close to
  // server.onclose before the process exits (verified — onclose never fires on
  // EOF), so beforeExit is the reliable hook for the normal client-disconnect
  // path; the signal handlers cover launchd/operator teardown; server.onclose
  // remains for an explicit server.close(). emitStopped is guarded so exactly
  // one path emits.
  let stopped = false;
  const emitStopped = (reason: string): void => {
    if (stopped) return;
    stopped = true;
    emitSpine('mcp.stdio.stopped', [`pid=${process.pid}`, `reason=${reason}`]);
  };
  server.onclose = () => emitStopped('transport-closed');
  process.on('beforeExit', () => emitStopped('stdin-closed'));
  process.on('SIGINT', () => { emitStopped('SIGINT'); process.exit(0); });
  process.on('SIGTERM', () => { emitStopped('SIGTERM'); process.exit(0); });

  await server.connect(transport);
  // operational visibility: record that a stdio session is up and serving.
  emitSpine('mcp.stdio.started', [`pid=${process.pid}`, 'transport=stdio']);
  // stays alive reading stdin; exits when stdin closes / process is signalled
}

// Run only when executed directly (tsx src/main-stdio.ts), not when imported
// by a test — otherwise main() would seize the test runner's stdin.
if (process.argv[1] && process.argv[1].includes('main-stdio')) {
  main().catch((err) => {
    const msg = err instanceof Error ? err.message : String(err);
    emitSpine('mcp.stdio.error', [`error_message=${msg.slice(0, 300)}`]);
     
    console.error('[chorus-mcp-stdio] fatal', msg);
    process.exit(1);
  });
}
