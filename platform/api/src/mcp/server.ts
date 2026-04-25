/**
 * #2472 — MCP server for chorus-api.
 *
 * Exposes Chorus operations as typed MCP tools so roles call them via Claude
 * Code's native tool-call surface instead of bash CLI / direct HTTP.
 *
 * First tool: chorus_nudge_message — delegates to the existing chorus-hook-shim
 * nudge invocation (which already emits nudge.emitted to spine canonically).
 * No new write paths; the bash CLI and the MCP tool both end up running the
 * same shim binary.
 *
 * Transport: Streamable HTTP (per MCP spec 2025-11-25). Mounted at POST /mcp
 * by server.ts. Per-role context comes from X-Chorus-Role request header,
 * falling back to CHORUS_ROLE env var.
 */
import { Server } from '@modelcontextprotocol/sdk/server/index.js';
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
} from '@modelcontextprotocol/sdk/types.js';
import { execFile } from 'child_process';
import { promisify } from 'util';
import { z } from 'zod';
import { resolveShimPath } from '../shim-path';

const NudgeInput = z.object({
  to: z.enum(['silas', 'wren', 'kade', 'jeff']).describe('Target role'),
  message: z.string().min(1).describe('Message text the recipient sees'),
});

export type NudgeArgs = z.infer<typeof NudgeInput>;

/** #2474 — async exec contract: takes a promisified execFile-shaped fn. */
export type ExecFileAsync = (
  file: string,
  args: string[],
  opts: { env?: NodeJS.ProcessEnv; timeout?: number },
) => Promise<{ stdout: string; stderr: string }>;

/** #2474 — DI seam for tests: inject mock execFile / fixed shim path. */
export interface McpServerDeps {
  execFileAsync?: ExecFileAsync;
  shimPath?: string;
}

const NUDGE_TOOL_DEF = {
  name: 'chorus_nudge_message',
  description:
    'Send a message to another Chorus role. Delivered to the role\'s active session best-effort within ~3s. Use this to coordinate work, ask a question, or notify of a state change. Do NOT use for batch broadcasts or non-actionable status — those belong on chorus-log. The sender is read from request context.',
  inputSchema: {
    type: 'object',
    properties: {
      to: {
        type: 'string',
        enum: ['silas', 'wren', 'kade', 'jeff'],
        description: 'Recipient — silas/wren/kade are AI roles, jeff is the human',
      },
      message: {
        type: 'string',
        minLength: 1,
        description: 'Message text the recipient sees',
      },
    },
    required: ['to', 'message'],
  },
} as const;

function logEvent(level: 'info' | 'error', event: string, fields: Record<string, unknown>): void {
  process.stderr.write(JSON.stringify({ level, event, tool: 'chorus_nudge_message', ts: new Date().toISOString(), ...fields }) + '\n');
}

async function executeNudge(
  args: NudgeArgs,
  from: string,
  execFileAsync: ExecFileAsync,
  shimPath: string,
): Promise<{ content: Array<{ type: 'text'; text: string }> }> {
  const { to, message } = args;
  logEvent('info', 'mcp.nudge.invoked', { from, to });
  try {
    const env = { ...process.env, DEPLOY_ROLE: from } as NodeJS.ProcessEnv;
    const { stdout } = await execFileAsync(shimPath, ['nudge', to, message], { env, timeout: 10_000 });
    logEvent('info', 'mcp.nudge.delivered', { from, to, stdout: stdout.slice(0, 200) });
    return { content: [{ type: 'text', text: `nudge sent: ${from} → ${to}` }] };
  } catch (err) {
    const errMsg = err instanceof Error ? err.message : String(err);
    logEvent('error', 'mcp.nudge.failed', { from, to, error: errMsg });
    throw new Error(`nudge delivery failed: ${errMsg}`);
  }
}

/**
 * Build the MCP server with one tool registered. Caller mounts a transport.
 * Caller passes a context-resolver that returns the sender role for a request
 * (read from header / env / session map). Keeps server module pure.
 */
export function buildMcpServer(getCallerRole: () => string, deps: McpServerDeps = {}): Server {
  const execFileAsync: ExecFileAsync = deps.execFileAsync ?? (promisify(execFile) as unknown as ExecFileAsync);
  const shimPath = deps.shimPath ?? resolveShimPath();
  const server = new Server(
    {
      name: 'chorus-api',
      version: '1.0.0',
    },
    {
      capabilities: { tools: {} },
    },
  );

  // eslint-disable-next-line @typescript-eslint/require-await -- MCP SDK requires async signature
  server.setRequestHandler(ListToolsRequestSchema, async () => ({ tools: [NUDGE_TOOL_DEF] }));

  server.setRequestHandler(CallToolRequestSchema, async (req) => {
    if (req.params.name !== 'chorus_nudge_message') {
      throw new Error(`Unknown tool: ${req.params.name}`);
    }
    const parsed = NudgeInput.safeParse(req.params.arguments);
    if (!parsed.success) {
      throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
    }
    return executeNudge(parsed.data, getCallerRole(), execFileAsync, shimPath);
  });

  return server;
}
