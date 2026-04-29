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

/** #2474 — DI seam for tests: inject mock execFile / fixed shim path.
 *  #2476 — extends with fetchImpl for tools that delegate to HTTP rather than
 *  spawning a binary (the principles tools call existing Athena REST). Default
 *  is the runtime's globalThis.fetch (Node 18+).
 */
export type FetchImpl = (url: string, init?: { method?: string; headers?: Record<string, string>; body?: string }) => Promise<{
  ok: boolean;
  status?: number;
  json: () => Promise<unknown>;
}>;

export interface McpServerDeps {
  execFileAsync?: ExecFileAsync;
  shimPath?: string;
  fetchImpl?: FetchImpl;
  apiBase?: string;
}

const PrinciplesGetInput = z.object({
  id: z.string().min(1).describe('Principle id (e.g., hemenway-observe, principle-ship-small)'),
});

const DecisionsGetInput = z.object({
  id: z.string().min(1).describe('Decision id (e.g., dec-2090, adr-026)'),
});
const PrinciplesCreateInput = z.object({
  label: z.string().min(1).describe('Short human-readable name (e.g., "Ship small")'),
  comment: z.string().optional().describe('One-paragraph description of the principle'),
  broaderOf: z.string().optional().describe('Optional parent principle id this derives from'),
  dcSource: z.string().optional().describe('Optional dc:source citation (book, ADR, etc.)'),
});

const PRINCIPLES_LIST_TOOL_DEF = {
  name: 'chorus_principles_list',
  description:
    'List all Chorus principles from the live graph. Use this to show every principle the team has agreed on (~46 today). Returns id + label + comment for each. Use to ground a decision in the canonical set or to discover what already exists before creating a new one. Do NOT use as a paraphrase target — cite by id, do not rewrite the text.',
  inputSchema: {
    type: 'object',
    properties: {},
    required: [],
  },
} as const;

const PRINCIPLES_GET_TOOL_DEF = {
  name: 'chorus_principles_get',
  description:
    'Get one Chorus principle by id from the live graph. Use this when you have a specific principle id (from a CLAUDE.md citation, a card, or chorus_principles_list) and want its full body. Returns label + comment + parent edges. Do NOT use to fish for a principle by topic — use chorus_principles_list and filter; ids are stable but labels are mutable.',
  inputSchema: {
    type: 'object',
    properties: {
      id: {
        type: 'string',
        minLength: 1,
        description: 'Principle id, e.g., hemenway-observe or principle-ship-small',
      },
    },
    required: ['id'],
  },
} as const;

const PRINCIPLES_CREATE_TOOL_DEF = {
  name: 'chorus_principles_create',
  description:
    'Create a new Chorus principle in the live graph. Use this only after the team has agreed a new principle is needed (rare — principles change slowly). Required: label. Optional: comment (one-paragraph description), broaderOf (parent principle id), dcSource (citation). Do NOT use for practices, policies, or skills — those have separate surfaces. Do NOT use to update an existing principle — there is no chorus_principles_update yet (PUT REST stays available).',
  inputSchema: {
    type: 'object',
    properties: {
      label: {
        type: 'string',
        minLength: 1,
        description: 'Short human-readable name',
      },
      comment: {
        type: 'string',
        description: 'One-paragraph description of the principle',
      },
      broaderOf: {
        type: 'string',
        description: 'Optional parent principle id this derives from',
      },
      dcSource: {
        type: 'string',
        description: 'Optional dc:source citation (book, ADR, etc.)',
      },
    },
    required: ['label'],
  },
} as const;

const DECISIONS_LIST_TOOL_DEF = {
  name: 'chorus_decisions_list',
  description:
    'List all Chorus decisions (DECs + ADRs) from the live graph. Use this to show every recorded team decision and architecture record (~140 today). Returns id + label + decisionType for each. Use to ground a position in the canonical record or to discover what already exists before re-arguing it. Do NOT use as a paraphrase target — cite by id, do not rewrite the body.',
  inputSchema: { type: 'object', properties: {}, required: [] },
} as const;

const DECISIONS_GET_TOOL_DEF = {
  name: 'chorus_decisions_get',
  description:
    'Get one Chorus decision (DEC or ADR) by id from the live graph. Use this when you have a specific id (from a CLAUDE.md citation, a card, or chorus_decisions_list) and want its full body. Returns label + comment + status (ADR only) + relatedCard. Do NOT use to fish for a decision by topic — use chorus_decisions_list and filter; ids are stable but labels are mutable.',
  inputSchema: {
    type: 'object',
    properties: {
      id: { type: 'string', minLength: 1, description: 'Decision id, e.g., adr-026 or dec-2090' },
    },
    required: ['id'],
  },
} as const;

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

// #2557 verification — DELETE ME after merge.
// Deliberately violates mcp-description-shape lint (description < 80 chars,
// no disposition guidance, no anti-pattern clause). The lint check is in
// the non-required tier post-#2526; this card proves a non-required failure
// merges green. Revert PR follows immediately. NOT registered in the tools
// array below — defined-but-orphan so production MCP surface is untouched.
const VERIFICATION_2557_TOOL_DEF = {
  name: 'chorus_verification_2557_delete_me',
  description: 'X',
  inputSchema: { type: 'object', properties: {} },
} as const;
void VERIFICATION_2557_TOOL_DEF; // satisfy ts noUnusedLocals

function logEvent(level: 'info' | 'error', event: string, fields: Record<string, unknown>): void {
  process.stderr.write(JSON.stringify({ level, event, tool: 'chorus_nudge_message', ts: new Date().toISOString(), ...fields }) + '\n');
}

interface PrincipleRecord {
  id: string;
  label?: string;
  comment?: string;
  techReading?: string;
  jeffReading?: string;
  isPermacultureParent?: boolean;
  parents?: string[];
  uri?: string;
}

async function fetchPrinciplesList(fetchImpl: FetchImpl, apiBase: string): Promise<PrincipleRecord[]> {
  const url = `${apiBase}/api/loom/principles`;
  const resp = await fetchImpl(url);
  if (!resp.ok) {
    throw new Error(`principles list fetch failed (status ${resp.status ?? 'unknown'})`);
  }
  const body = (await resp.json()) as { data?: { principles?: PrincipleRecord[] } };
  return body.data?.principles ?? [];
}

async function executePrinciplesList(
  fetchImpl: FetchImpl,
  apiBase: string,
  from: string,
): Promise<{ content: Array<{ type: 'text'; text: string }> }> {
  process.stderr.write(JSON.stringify({ level: 'info', event: 'mcp.principles.list.invoked', tool: 'chorus_principles_list', from, ts: new Date().toISOString() }) + '\n');
  const principles = await fetchPrinciplesList(fetchImpl, apiBase);
  const lines: string[] = [`${principles.length} principle${principles.length === 1 ? '' : 's'}:`];
  for (const p of principles) {
    const label = p.label ? `${p.label} (${p.id})` : p.id;
    const summary = p.comment ? `${label} — ${p.comment}` : label;
    lines.push(`- ${summary}`);
  }
  return { content: [{ type: 'text', text: lines.join('\n') }] };
}

async function executePrinciplesGet(
  args: { id: string },
  fetchImpl: FetchImpl,
  apiBase: string,
  from: string,
): Promise<{ content: Array<{ type: 'text'; text: string }> }> {
  // TODO(#2476-followon): Athena has no single-principle GET endpoint; we
  // fetch the full list (~46) and filter in memory. Fine at this scale, but
  // when an Athena GET /api/athena/subdomains/loom-principles/principles/:id
  // lands, swap to it for O(1) and to avoid pulling the full set on every
  // get call. Code smell flagged in #2476 gate:code review (kade).
  process.stderr.write(JSON.stringify({ level: 'info', event: 'mcp.principles.get.invoked', tool: 'chorus_principles_get', from, id: args.id, ts: new Date().toISOString() }) + '\n');
  const principles = await fetchPrinciplesList(fetchImpl, apiBase);
  const found = principles.find((p) => p.id === args.id);
  if (!found) {
    throw new Error(`principle not found: ${args.id}`);
  }
  const lines = [
    `${found.label ?? found.id} (${found.id})`,
    '',
    found.comment ?? '(no comment)',
  ];
  if (found.parents && found.parents.length > 0) {
    lines.push('', `parents: ${found.parents.join(', ')}`);
  }
  if (found.uri) {
    lines.push(`uri: ${found.uri}`);
  }
  return { content: [{ type: 'text', text: lines.join('\n') }] };
}

async function executePrinciplesCreate(
  args: { label: string; comment?: string; broaderOf?: string; dcSource?: string },
  fetchImpl: FetchImpl,
  apiBase: string,
  from: string,
): Promise<{ content: Array<{ type: 'text'; text: string }> }> {
  process.stderr.write(JSON.stringify({ level: 'info', event: 'mcp.principles.create.invoked', tool: 'chorus_principles_create', from, label: args.label, ts: new Date().toISOString() }) + '\n');
  const url = `${apiBase}/api/athena/subdomains/loom-principles/principles`;
  const body: Record<string, string> = { label: args.label };
  if (args.comment) body.comment = args.comment;
  if (args.broaderOf) body.broaderOf = args.broaderOf;
  if (args.dcSource) body.dcSource = args.dcSource;
  const resp = await fetchImpl(url, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  if (!resp.ok) {
    throw new Error(`principles create failed (status ${resp.status ?? 'unknown'})`);
  }
  const result = (await resp.json()) as { data?: { id?: string; uri?: string } };
  const id = result.data?.id ?? result.data?.uri ?? '(unknown id)';
  return { content: [{ type: 'text', text: `principle created: ${id}` }] };
}

interface DecisionRecord {
  id: string;
  label?: string;
  comment?: string;
  decisionType?: string;
  status?: string;
  relatedCard?: number | string;
  uri?: string;
}

async function fetchDecisionsList(fetchImpl: FetchImpl, apiBase: string): Promise<DecisionRecord[]> {
  // Hit Athena subdomain handler directly (loom alias 308-redirects here;
  // skipping the redirect hop matches where the data flows).
  const url = `${apiBase}/api/athena/subdomains/loom-decisions/decisions`;
  const resp = await fetchImpl(url);
  if (!resp.ok) {
    throw new Error(`decisions list fetch failed (status ${resp.status ?? 'unknown'})`);
  }
  const body = (await resp.json()) as { data?: { decisions?: DecisionRecord[] } };
  return body.data?.decisions ?? [];
}

async function executeDecisionsList(
  fetchImpl: FetchImpl,
  apiBase: string,
  from: string,
): Promise<{ content: Array<{ type: 'text'; text: string }> }> {
  process.stderr.write(JSON.stringify({ level: 'info', event: 'mcp.decisions.list.invoked', tool: 'chorus_decisions_list', from, ts: new Date().toISOString() }) + '\n');
  const decisions = await fetchDecisionsList(fetchImpl, apiBase);
  const lines: string[] = [`${decisions.length} decision${decisions.length === 1 ? '' : 's'}:`];
  for (const d of decisions) {
    const kind = d.decisionType ? `[${d.decisionType}] ` : '';
    const label = d.label ? `${kind}${d.label} (${d.id})` : `${kind}${d.id}`;
    lines.push(`- ${label}`);
  }
  return { content: [{ type: 'text', text: lines.join('\n') }] };
}

async function executeDecisionsGet(
  args: { id: string },
  fetchImpl: FetchImpl,
  apiBase: string,
  from: string,
): Promise<{ content: Array<{ type: 'text'; text: string }> }> {
  process.stderr.write(JSON.stringify({ level: 'info', event: 'mcp.decisions.get.invoked', tool: 'chorus_decisions_get', from, id: args.id, ts: new Date().toISOString() }) + '\n');
  const decisions = await fetchDecisionsList(fetchImpl, apiBase);
  const found = decisions.find((d) => d.id === args.id);
  if (!found) throw new Error(`decision not found: ${args.id}`);
  const lines = [
    `${found.label ?? found.id} (${found.id})`,
    '',
    found.comment ?? '(no comment)',
  ];
  if (found.status) lines.push('', `status: ${found.status}`);
  if (found.relatedCard !== undefined) lines.push(`relatedCard: #${found.relatedCard}`);
  if (found.uri) lines.push(`uri: ${found.uri}`);
  return { content: [{ type: 'text', text: lines.join('\n') }] };
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
    const env = {
      ...process.env,
      DEPLOY_ROLE: from,
      // #2475 — origin tag distinguishes MCP-routed nudges from bash CLI in
      // the spine. nudge.emitted carries origin=mcp so audit can tell typed
      // surface adoption from legacy paths.
      CHORUS_NUDGE_ORIGIN: 'mcp',
    } as NodeJS.ProcessEnv;
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
  const fetchImpl: FetchImpl = deps.fetchImpl ?? (globalThis.fetch as unknown as FetchImpl);
  const apiBase = deps.apiBase ?? 'http://localhost:3340';
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
  server.setRequestHandler(ListToolsRequestSchema, async () => ({
    tools: [
      NUDGE_TOOL_DEF,
      PRINCIPLES_LIST_TOOL_DEF,
      PRINCIPLES_GET_TOOL_DEF,
      PRINCIPLES_CREATE_TOOL_DEF,
      DECISIONS_LIST_TOOL_DEF,
      DECISIONS_GET_TOOL_DEF,
    ],
  }));

  server.setRequestHandler(CallToolRequestSchema, async (req) => {
    const from = getCallerRole();
    switch (req.params.name) {
      case 'chorus_nudge_message': {
        const parsed = NudgeInput.safeParse(req.params.arguments);
        if (!parsed.success) {
          throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
        }
        return executeNudge(parsed.data, from, execFileAsync, shimPath);
      }
      case 'chorus_principles_list':
        return executePrinciplesList(fetchImpl, apiBase, from);
      case 'chorus_principles_get': {
        const parsed = PrinciplesGetInput.safeParse(req.params.arguments);
        if (!parsed.success) {
          throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
        }
        return executePrinciplesGet(parsed.data, fetchImpl, apiBase, from);
      }
      case 'chorus_principles_create': {
        const parsed = PrinciplesCreateInput.safeParse(req.params.arguments);
        if (!parsed.success) {
          throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
        }
        return executePrinciplesCreate(parsed.data, fetchImpl, apiBase, from);
      }
      case 'chorus_decisions_list':
        return executeDecisionsList(fetchImpl, apiBase, from);
      case 'chorus_decisions_get': {
        const parsed = DecisionsGetInput.safeParse(req.params.arguments);
        if (!parsed.success) {
          throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
        }
        return executeDecisionsGet(parsed.data, fetchImpl, apiBase, from);
      }
      default:
        throw new Error(`Unknown tool: ${req.params.name}`);
    }
  });

  return server;
}
