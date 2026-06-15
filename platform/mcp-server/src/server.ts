/* eslint-disable security/detect-object-injection -- MCP tool dispatch is keyed by tool name from a controlled, in-process tool registry (not untrusted input); the indexed access is over our own definition tables (#3429) */
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
import { resolveShimPath } from './shim-path';
import { resolveCardsPath } from './cards-path';
import { queryLogs, recentErrors, logsForCard, logsForTrace, logsForBranch, type LogsQueryDeps } from './handlers/logs-query';
import { executeDesignRefresh } from './design-refresh';
// #2997 — athena-tree handler stays in chorus-api for now (heavy fuseki deps).
// chorus-mcp calls it via HTTP from chorus-api instead of importing in-process.
// This keeps chorus-mcp's surface minimal — only depends on cards CLI, git-queue,
// chorus-hook-shim, and Loki HTTP. No fuseki client, no oxigraph, no lancedb.
import {
  loadTree as athenaLoadTree,
  getTree as athenaGetTree,
  lookupOwnership as athenaLookupOwnership,
  computeBlastRadius as athenaComputeBlastRadius,
} from './athena-tree-stub';

const NudgeInput = z.object({
  to: z.enum(['silas', 'wren', 'kade', 'jeff']).describe('Target role'),
  message: z.string().min(1).describe('Message text the recipient sees'),
  // #3403 — what the sender needs back. Default 'none' (fyi/ack, never traps).
  // 'reply'/'decision'/'action' make the recipient owe a response (gated).
  expects: z.enum(['none', 'reply', 'decision', 'action']).optional().describe('What you need back; set reply/decision/action to require a response'),
});

export type NudgeArgs = z.infer<typeof NudgeInput>;

/** #2474 — async exec contract: takes a promisified execFile-shaped fn.
 *  #2662 — opts gains `cwd` so callers spawning binaries that run relative
 *  to repo root (e.g., chorus_commit → git-queue.sh staging paths) can set
 *  it explicitly. Without this, chorus-api's process cwd (platform/api)
 *  leaked into git add and broke path resolution.
 */
export type ExecFileAsync = (
  file: string,
  args: string[],
  opts: { env?: NodeJS.ProcessEnv; timeout?: number; cwd?: string; maxBuffer?: number },
) => Promise<{ stdout: string; stderr: string }>;

/** #2474 — DI seam for tests: inject mock execFile / fixed shim path.
 *  #2476 — extends with fetchImpl for tools that delegate to HTTP rather than
 *  spawning a binary (the principles tools call existing Athena REST). Default
 *  is the runtime's globalThis.fetch (Node 18+).
 */
export type FetchImpl = (url: string, init?: { method?: string; headers?: Record<string, string>; body?: string; signal?: AbortSignal }) => Promise<{
  ok: boolean;
  status?: number;
  json: () => Promise<unknown>;
  text?: () => Promise<string>;
}>;

export interface McpServerDeps {
  execFileAsync?: ExecFileAsync;
  shimPath?: string;
  cardsPath?: string;
  fetchImpl?: FetchImpl;
  apiBase?: string;
  // #2661 — board reader DI seam. Returns cards matching (owner, status=WIP).
  // Default impl fetches /api/chorus/context/board/wip and filters by owner.
  // Tests inject mocks to drive refusal taxonomy without standing up chorus-api.
  boardReader?: BoardReader;
  // #2661 — spine event emitter DI seam. Default writes a JSON line to
  // stderr (same channel as logEvent); tests inject a capture function.
  emitSpineEvent?: SpineEmitter;
  // #2913 — resolve the working tree (cwd / script root) for a role. The
  // default impl globs chorus-werk/<role>-* : a single match is the role's
  // active card werk, zero/ambiguous returns canonical (#2662 cwd=repo-root
  // contract preserved). Tests inject a stub path.
  resolveWorkingTree?: (role: 'kade' | 'wren' | 'silas') => string;
  // #2760 — werk path existence check. Default uses fs.existsSync; tests
  // inject `() => true` so refusal taxonomy tests don't need real /tmp dirs.
  fsExists?: (p: string) => boolean;
}

export type BoardCard = { id: number; owner: string; title: string };
export type BoardReaderResult =
  | { ok: true; cards: BoardCard[] }
  | { ok: false; reason: 'board-unreachable'; detail?: string };
export type BoardReader = (role: 'kade' | 'wren' | 'silas') => Promise<BoardReaderResult>;
export type SpineEmitter = (event: string, fields: Record<string, unknown>) => void;

// #2652 (AC8) — cards MCP tool input schemas. Each tool spawns the cards bash
// wrapper as a subprocess with DEPLOY_ROLE injected. Args are positional per
// the cards CLI; tools translate structured MCP arguments to argv.
const CardsAddInput = z.object({
  title: z.string().min(1).describe('Short imperative card title'),
  owner: z.enum(['wren', 'silas', 'kade', 'jeff']).describe('Owner role'),
  priority: z.enum(['P1', 'P2', 'P3']).describe('Priority — P1 highest'),
  domain: z.string().min(1).describe('Domain label (e.g., chorus, photos, seeds)'),
  type: z.enum(['new', 'enhance', 'fix', 'chore', 'swat']).describe('Card type'),
  origin: z.enum(['reflective', 'reactive']).describe('Origin — reflective=chosen, reactive=responding to breakage'),
  desc: z.string().min(1).describe('Card description (Experience + AC, markdown)'),
  sequence: z.string().optional().describe('Sequence label (deprecated by subproduct per #2643)'),
  chunk: z.string().optional().describe('Optional chunk (app, ops, memory, ...)'),
  subproduct: z.enum(['athena', 'loom', 'werk', 'borg', 'convergence', 'clearing']).optional().describe('Subproduct — implementation within Chorus (#2652 AC2)'),
  subdomain: z.string().optional().describe('Subdomain — Athena subdomain id, refused-at-source against live /api/athena/subdomains (#2652 AC1)'),
});

// #2996 — Jeff-attributed card add. Same fields as CardsAddInput. #3293: the CLI
// now enforces the Experience+AC floor on every card (no --quick); Jeff cards skip
// only the agent six-section gate (via attribution), so desc is effectively required.
const CardAddJeffInput = z.object({
  title: z.string().min(1).describe('Short imperative card title'),
  owner: z.enum(['wren', 'silas', 'kade', 'jeff']).describe('Owner role'),
  priority: z.enum(['P1', 'P2', 'P3']).describe('Priority — P1 highest'),
  domain: z.string().min(1).describe('Domain label (e.g., chorus, photos, seeds)'),
  type: z.enum(['new', 'enhance', 'fix', 'chore', 'swat']).describe('Card type'),
  origin: z.enum(['reflective', 'reactive']).describe('Origin — reflective=chosen, reactive=responding to breakage'),
  desc: z.string().optional().describe('Optional description (Jeff-initiated cards skip the six-section gate)'),
  sequence: z.string().optional().describe('Sequence label (deprecated by subproduct per #2643)'),
  chunk: z.string().optional().describe('Optional chunk (app, ops, memory, ...)'),
  subproduct: z.enum(['athena', 'loom', 'werk', 'borg', 'convergence', 'clearing']).optional().describe('Subproduct — implementation within Chorus (#2652 AC2)'),
  subdomain: z.string().optional().describe('Subdomain — Athena subdomain id (#2652 AC1)'),
});

const CardsMoveInput = z.object({
  id: z.number().int().positive().describe('Card id'),
  status: z.enum(['Now', 'Next', 'Later', 'WIP', 'Blocked', 'Done', 'Won\'t Do', 'Harvesting', 'SWAT']).describe('New status'),
});

const CardsDoneInput = z.object({
  id: z.number().int().positive().describe('Card id to mark Done'),
});

const CardsTagInput = z.object({
  id: z.number().int().positive().describe('Card id'),
  category: z.enum(['sequence', 'domain', 'chunk']).describe('Tag axis'),
  value: z.string().min(1).describe('Tag value (e.g., werk, chorus, app)'),
  op: z.enum(['add', 'remove']).default('add').describe('add (default) or remove'),
});

const CardsSetInput = z.object({
  id: z.number().int().positive().describe('Card id'),
  fields: z.record(z.string(), z.string()).describe('Field=value pairs (e.g., {priority: "P1", owner: "wren"})'),
});

const CardsViewInput = z.object({
  id: z.number().int().positive().describe('Card id to view'),
});

// #2661 — chorus_commit_status MCP tool input. Single role field; per the
// commits-service-design v3 contract, no card_id / branch / force / bypass
// on the wire. Service derives the active card from the BOARD (#2467/#2629:
// card lives on the board, role-state owns session/attention only).
const CommitStatusInput = z.object({
  role: z.enum(['kade', 'wren', 'silas']).describe('Calling role — kade/wren/silas. Service queries the board for this role\'s active WIP card.'),
}).strict();

// #3178 — werk-commit input (v1 chorus_commit/git-queue.sh contract CUT). Card-
// scoped: the rust werk-commit stages the card's werk changes (add -A in the
// ephemeral werk, which IS the card's file set) and formats the message
// "<role>: #<card> — <summary>". No explicit `paths` — that was the v1 contract.
const CommitInput = z.object({
  role: z.enum(['kade', 'wren', 'silas']).describe('Builder role — owns the werk <role>/<card> being committed.'),
  card_id: z.number().int().positive().describe('Card ID whose werk changes to commit.'),
  summary: z.string().min(1).optional().describe('Optional short summary; werk-commit formats the message as "<role>: #<card> — <summary>".'),
}).strict();

// #2751 — chorus_pull_card atomic transaction input. Role + explicit card_id;
// the /pull skill is the caller, and Jeff or the role names which card.
// No bypasses on the wire — werk-dirty / werk-wrong-branch are typed refusals,
// not flags the caller can suppress.
const PullCardInput = z.object({
  role: z.enum(['kade', 'wren', 'silas']).describe('Calling role — kade/wren/silas. DEPLOY_ROLE attribution + spine event role field.'),
  card_id: z.number().int().positive().describe('Card ID to pull. Must be in Next or Later status with AC + Experience populated.'),
}).strict();

// #3178 — werk-push input. Thin skin over the rust werk-push verb.
const WerkPushInput = z.object({
  role: z.enum(['kade', 'wren', 'silas']).describe('Builder role — owns the werk <role>/<card> being pushed.'),
  card_id: z.number().int().positive().describe('Card ID whose werk branch to push.'),
}).strict();

// #3319 — loom-gemba input. Observation is watcher→target, not card-scoped.
const LoomGembaInput = z.object({
  role: z.enum(['kade', 'wren', 'silas']).describe('Observer role — who is watching. DEPLOY_ROLE attribution + the observing state declared.'),
  target: z.enum(['kade', 'wren', 'silas']).describe('Target role being observed.'),
}).strict();

// #3175 — werk-merge input. Thin skin over the rust werk-merge verb.
const WerkMergeInput = z.object({
  role: z.enum(['kade', 'wren', 'silas']).describe('Builder role — owns the werk branch <role>/<card> being merged to main.'),
  card_id: z.number().int().positive().describe('Card ID whose pushed branch to merge.'),
}).strict();

// #3178 — werk-accept input. `role` is the BUILDER (werk location); the ACCEPTER
// is the calling identity (DEPLOY_ROLE), set by the handler from getCallerRole —
// only jeff/wren may finalize (DEC-048).
const WerkAcceptInput = z.object({
  role: z.enum(['kade', 'wren', 'silas']).describe('Builder role whose card/werk is being accepted (werk location).'),
  card_id: z.number().int().positive().describe('Card ID to finalize.'),
}).strict();

// #2900 — chorus_design_refresh input. Refreshes cite-density layers of a
// service-design HTML from current card statuses. Skill body is one MCP
// call; same substrate pattern as /acp + /pull.
const DesignRefreshInput = z.object({
  role: z.enum(['kade', 'wren', 'silas']).describe('Calling role — kade/wren/silas. DEPLOY_ROLE attribution + spine event role field.'),
  design_name: z.string().min(1).describe('Filename stem (or basename) of the service design HTML, e.g. "build-and-deploy-service-design". Looked up under designing/docs/<name>.html.'),
}).strict();

// #2969 — chorus_doc_catalog_add input. Registers a doc in the catalog so it
// surfaces in the Athena UI. Thin typed wrapper around POST /api/doc-catalog/add
// → registerDoc() in handlers/doc-catalog.ts (the canonical writer to the
// registry JSON). Closes the no-typed-surface gap that drove the canonical
// JSON-edit bypass demonstrated on 2026-05-17.
const DocCatalogAddInput = z.object({
  filePath: z.string().min(1).describe('Absolute path to the .html or .md file to register. Must exist on disk; the handler refuses with file-not-found otherwise.'),
  href: z.string().min(1).describe('Public href the Athena UI uses to reach the doc (e.g., "/gathering-docs/awareness-service-design.html" or "/loom/principles.html"). Must be unique in the registry; refuses with already-registered on duplicate.'),
  group: z.string().optional().describe('Optional logical grouping label (free text). Used by the UI for visual clustering; not load-bearing.'),
}).strict();

// #2759 — chorus_unpull_card atomic teardown input. /pull's natural inverse.
// Role + card_id; same shape as pull. Refuses if card isn't WIP-owned-by-role
// or werk has uncommitted work (don't lose work).
const UnpullCardInput = z.object({
  role: z.enum(['kade', 'wren', 'silas']).describe('Calling role — kade/wren/silas. DEPLOY_ROLE attribution + spine event role field.'),
  card_id: z.number().int().positive().describe('Card ID to unpull. Must be currently WIP and owned by role.'),
}).strict();

const PrinciplesGetInput = z.object({
  id: z.string().min(1).describe('Principle id (e.g., hemenway-observe, principle-ship-small)'),
});

const DecisionsGetInput = z.object({
  id: z.string().min(1).describe('Decision id (e.g., dec-2090, adr-026)'),
});
// eslint-disable-next-line @typescript-eslint/no-unused-vars -- parked subdomains tool, intentionally retained for future wiring (see ~L2560) (#3429)
const SubdomainsGetInput = z.object({
  id: z.string().min(1).describe('Subdomain id (e.g., commits-domain, gates-service)'),
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

// eslint-disable-next-line @typescript-eslint/no-unused-vars -- parked subdomains tool def, intentionally retained for future wiring (#3429)
const SUBDOMAINS_LIST_TOOL_DEF = {
  name: 'chorus_subdomains_list',
  description:
    'List all Chorus subdomains from the live Athena graph. Returns id + label + owner + step for each. Use this to look up current ownership when DEC-058 says to query Athena via MCP — the canonical model is the source of truth, not a hardcoded table. Do NOT cache locally; ownership shifts as cards land.',
  inputSchema: {
    type: 'object',
    properties: {},
    required: [],
  },
} as const;

// eslint-disable-next-line @typescript-eslint/no-unused-vars -- parked subdomains tool def, intentionally retained for future wiring (#3429)
const SUBDOMAINS_GET_TOOL_DEF = {
  name: 'chorus_subdomains_get',
  description:
    'Get one Chorus subdomain by id from the live Athena graph. Use when you have a specific subdomain id (e.g., commits-domain, gates-service) and want its full record — id, label, owner, step. Do NOT use to fish by topic; use chorus_subdomains_list and filter.',
  inputSchema: {
    type: 'object',
    properties: {
      id: {
        type: 'string',
        minLength: 1,
        description: 'Subdomain id, e.g., commits-domain or gates-service',
      },
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
      expects: {
        type: 'string',
        enum: ['none', 'reply', 'decision', 'action'],
        description:
          'What you need back (#3403). Default \'none\' — a fyi/ack the recipient need not answer. Set \'reply\' (or \'decision\'/\'action\') ONLY when you genuinely need a response: it makes the recipient owe you one — their session is gently held from other work until they answer. Use sparingly; a forgotten \'none\' just means no guaranteed reply, so re-nudge if needed.',
      },
    },
    required: ['to', 'message'],
  },
} as const;

// #2605 — service-lifecycle MCP tool defs. 6 per-verb tools wrapping
// agent-state.sh. Pattern matches chorus_acp/chorus_pull_card/chorus_commit
// shape (one tool = one verb = one typed-refusal taxonomy).
const ServiceLifecycleInput = z.object({
  service: z.string().min(1).describe('Service or crate name (e.g. chorus-api, chorus-hooks, com.chorus.api)'),
});
type ServiceVerb = 'status' | 'start' | 'stop' | 'restart' | 'deploy' | 'rollback';
const SERVICE_LIFECYCLE_VERBS: ReadonlyArray<ServiceVerb> = ['status', 'start', 'stop', 'restart', 'deploy', 'rollback'];

// #3110: werk-binary MCP wrapper inputs.
const RoleEnum = z.enum(['kade', 'wren', 'silas']);
const BuildInput = z.object({
  role: RoleEnum,
  card_id: z.number().int().min(1).describe('Card id whose werk holds the source.'),
});
const DeployInput = z.object({
  role: RoleEnum,
  card_id: z.number().int().min(1).describe('Card id whose werk holds the build artifacts.'),
  target: z.enum(['canonical', 'werk']).optional().describe('Install target. Default: canonical.'),
  // #3311 — env-up folded in (chorus_env_up tool deleted): werk-deploy env-up is a
  // subcommand of the same binary; one MCP name per binary. #3239 card_id forwarding kept.
  env_up: z.boolean().optional().describe('Bring up the role variant (werk-deploy env-up) instead of installing.'),
});
// #3241 — the whole werk pipeline as ONE MCP verb. Wraps the act run of werk.yml so the
// pipeline trigger is MCP like every other verb (no raw `act` CLI surface). accepter is
// for the PRINTED stop-before-accept command only — the verb never auto-accepts (DEC-048).
const WerkRunInput = z.object({
  role: RoleEnum,
  card_id: z.number().int().min(1).describe('Card to run through the pipeline.'),
  accepter: z.enum(['jeff', 'wren', 'kade', 'silas']).optional().describe('Authorizing identity (DEC-048). Default jeff. With go:true, who the accept runs under.'),
  // #3311 — ONE trigger: go=false/absent runs to the demo stop (werk.yml); go=true
  // resumes past it (werk.yml's go-gated `land` job: merge → sync → deploy → accept). GO = accept.
  go: z.boolean().optional().describe('The human GO — resume past the demo stop.'),
});

const SERVICE_STATUS_TOOL_DEF = {
  name: 'chorus_service_status',
  description: 'Use this to read the current launchd state of a chorus service — PID, exit code, running cdhash. Wraps `agent-state.sh status <svc>`. Refusal taxonomy: service-not-found | label-resolve-fail. Read-only verb, open to any role. Do NOT use to mutate state (start/stop/restart/deploy/rollback are write verbs) or to query historical lifecycle events (chorus_logs_for_card is the trace surface).',
  inputSchema: { type: 'object', properties: { service: { type: 'string', minLength: 1, description: 'Service or crate name (e.g. chorus-api, chorus-hooks, com.chorus.api)' } }, required: ['service'] },
} as const;

const SERVICE_START_TOOL_DEF = {
  name: 'chorus_service_start',
  description: 'Use this to start a chorus service that is currently stopped — restores a daemon to running state, emits paired service.start.{started,completed,failed} for trace. Wraps `agent-state.sh start <svc>`. Refusal taxonomy: service-not-found | already-running | bootstrap-fail. Write verb gated to silas by default (DEC-022). Do NOT use to recover from a crash (launchd KeepAlive auto-restarts) or to redeploy a new binary (chorus_service_deploy is the correct verb).',
  inputSchema: { type: 'object', properties: { service: { type: 'string', minLength: 1, description: 'Service name' } }, required: ['service'] },
} as const;

const SERVICE_STOP_TOOL_DEF = {
  name: 'chorus_service_stop',
  description: 'Use this to deliberately stop a chorus service — bootout the launchd job. Emits paired service.stop.{started,completed,failed}. Wraps `agent-state.sh stop <svc>`. Refusal taxonomy: service-not-found | already-stopped | bootout-fail. Write verb gated to silas. Do NOT use to kill a hung process (launchctl kickstart -k is the right primitive for restart) or as a step in a deploy (chorus_service_deploy handles the kickstart internally).',
  inputSchema: { type: 'object', properties: { service: { type: 'string', minLength: 1, description: 'Service name' } }, required: ['service'] },
} as const;

const SERVICE_RESTART_TOOL_DEF = {
  name: 'chorus_service_restart',
  description: 'Use this to restart a chorus service while preserving the installed binary — stop then start, cdhash check across the transition. Emits paired service.restart.{started,completed,failed} plus service.verify.divergence on cdhash change. Wraps `agent-state.sh restart <svc>`. Refusal taxonomy: service-not-found | kickstart-fail | verify-fail. Write verb gated to silas. Do NOT use to pick up a newly-installed binary (use chorus_service_deploy which builds + installs + verifies in one flow).',
  inputSchema: { type: 'object', properties: { service: { type: 'string', minLength: 1, description: 'Service name' } }, required: ['service'] },
} as const;

const SERVICE_DEPLOY_TOOL_DEF = {
  name: 'chorus_service_deploy',
  description: 'Use this to deploy a chorus crate end-to-end: `werk-deploy crate <name>` (the native engine, #3317) + launchctl kickstart + cdhash verify in one atomic flow. Emits paired service.deploy.{started,completed,failed}. Wraps `agent-state.sh deploy <crate>`. Refusal taxonomy: service-not-found | build-fail | kickstart-fail | cdhash-divergence | verify-timeout. Write verb with per-unit authority per #2927 (chorus-api → kade, chorus-hooks → silas, cards-sdk → wren). Do NOT use as part of the land flow (the verb pipeline deploys canonical in-flow) or to install without rebuilding (no such variant exists — every deploy rebuilds from current werk state).',
  inputSchema: { type: 'object', properties: { service: { type: 'string', minLength: 1, description: 'Crate name (chorus-api, chorus-hooks, chorus-inject)' } }, required: ['service'] },
} as const;

const SERVICE_ROLLBACK_TOOL_DEF = {
  name: 'chorus_service_rollback',
  description: 'Use this to roll back a chorus crate to the prior cdhash from manifest — restore the previous binary, kickstart, verify. Emits paired service.rollback.{started,completed,failed}. Wraps `agent-state.sh rollback <crate>` which invokes `werk-deploy crate <crate> --rollback` (#3317). Refusal taxonomy: service-not-found | no-prior-cdhash | restore-fail | kickstart-fail | verify-fail. Write verb with per-unit authority per #2927. Do NOT use as a substitute for `git revert` (rollback only restores binary; source state stays at HEAD) or to roll back further than one step (manifest holds only the immediately-prior cdhash today).',
  inputSchema: { type: 'object', properties: { service: { type: 'string', minLength: 1, description: 'Crate name to roll back' } }, required: ['service'] },
} as const;

// #3110: werk-binary MCP wrappers — werk-build / werk-deploy (#3310/#3311 renames; env-up is werk-deploy {env_up:true})
// expose werk-build, werk-deploy, and werk-deploy env-up as MCP tools so
// werk-demo (Wren's binary) consumes them via MCP instead of shelling out
// directly. Jeff: "no CLI shell-outs from werk-demo." Each wraps the
// corresponding verb binary in ~/.chorus/bin/ with role + card env wiring.
const CHORUS_BUILD_TOOL_DEF = {
  // #3310 — renamed chorus_build → werk-build (ADR-031/032: the verb family is werk-<verb>;
  // no chorus_-prefixed pseudo-verb). chorus_build removed, not aliased.
  name: 'werk-build',
  description: 'Use this to run werk-build for a card from the role\'s werk. Invokes ~/.chorus/bin/werk-build with role + card env, returning structured exit + stdout + stderr. Wraps the verb binary; per #3107 the build returns Ok-empty on no-build-cycle (docs/config/graph-only cards) rather than refusing. Use from werk-demo or any orchestrator that needs to drive a card\'s build without subprocess-shelling. Refusal taxonomy: usage-error | no-werk | branch-mismatch | build-fail. Do NOT use to deploy (werk-deploy).',
  inputSchema: {
    type: 'object',
    properties: {
      role: { type: 'string', enum: ['kade', 'wren', 'silas'], description: 'Calling role — drives DEPLOY_ROLE + werk path resolution.' },
      card_id: { type: 'integer', minimum: 1, description: 'Card id whose werk holds the source to build.' },
    },
    required: ['role', 'card_id'],
    additionalProperties: false,
  },
} as const;

const CHORUS_DEPLOY_TOOL_DEF = {
  // #3311 — renamed chorus_deploy → werk-deploy (same ADR-031/032 family cleanup).
  name: 'werk-deploy',
  description: 'Use this to run werk-deploy for a card from the role\'s werk. Invokes ~/.chorus/bin/werk-deploy with role + card env. Target defaults to canonical; pass target=werk to install to the per-card werk\'s target/release/ (variant deploy for staging). Wraps the verb binary. Refusal taxonomy: usage-error | no-werk | branch-mismatch | no-deploy-target (class 5, still open as of 2026-05-28) | install-fail | cdhash-divergence | verify-timeout. Use from werk-demo. Do NOT use to deploy services system-wide (chorus_service_deploy). Variant env-up: pass {env_up:true}.',
  inputSchema: {
    type: 'object',
    properties: {
      role: { type: 'string', enum: ['kade', 'wren', 'silas'], description: 'Calling role.' },
      card_id: { type: 'integer', minimum: 1, description: 'Card id whose werk holds the build artifacts to deploy.' },
      target: { type: 'string', enum: ['canonical', 'werk'], description: 'Install target — canonical (~/.chorus/bin/) or werk (per-card slot). Default: canonical.' },
    },
    required: ['role', 'card_id'],
    additionalProperties: false,
  },
} as const;

// #3311 — chorus_env_up DELETED: env-up is a werk-deploy subcommand; call the
// werk-deploy tool with {env_up:true}. One name per binary.

// #3241/#3311 — chorus_werk: the ONE pipeline trigger. Encapsulates the act run
// of werk.yml so callers never touch the raw `act` CLI (no -W/-P/--input, no PATH
// wrangling). The single invocation surface for the pipeline trigger, conforming to
// the MCP-verb contract — same way roles call werk-pull/werk-commit/chorus_build.
const CHORUS_WERK_TOOL_DEF = {
  // #3311 — ONE trigger, not two. chorus_werk runs the verb sequence to the demo
  // stop-point; chorus_werk {go:true} resumes past it on the human GO. The composite
  // pseudo-verbs (chorus_werk_land, and briefly werk-present/werk-land) are DELETED —
  // the werk- namespace is the seven verbs only.
  name: 'chorus_werk',
  description: 'THE pipeline trigger — the verb sequence with a stop at the demo. Default (no go): runs commit→push→build→test→deploy-werk→env-up→demo via act (werk.yml), PRESENTS the running variant, and STOPS (#3279) — returns in minutes with {ok, phase:"presented"}; nothing is held across the human wait. With go:true (ONLY on Jeff/Wren\'s explicit GO for a presented card): resumes past the stop via act (the same werk.yml, go-gated `land` job) — werk-merge → werk-deploy --target canonical → werk-accept. GO = accept (DEC-048): the accepter named here is the authority werk-accept runs under. Do NOT shell out to `act` directly, and never pass go:true without the human\'s explicit go.',
  inputSchema: {
    type: 'object',
    properties: {
      role: { type: 'string', enum: ['kade', 'wren', 'silas'], description: 'Builder role whose werk runs.' },
      card_id: { type: 'integer', minimum: 1, description: 'Card to run.' },
      accepter: { type: 'string', enum: ['jeff', 'wren', 'kade', 'silas'], description: 'Authorizing identity (DEC-048). Default jeff. With go:true this is who the accept runs under.' },
      go: { type: 'boolean', description: 'The human GO. false/absent = run to the demo stop and present. true = resume past the stop: merge → deploy-prod → accept.' },
    },
    required: ['role', 'card_id'],
    additionalProperties: false,
  },
} as const;

// #3269 — the card cycle/step/error fitness function as a standing instrument.
const FLOW_REPORT_TOOL_DEF = {
  name: 'chorus_flow_report',
  description: 'Use this to get the current flow-fitness of the system: per-card cycle time, per-step times (work/push/build/deploy/demo/merge/final), errors/warnings enumerated per card, and error classes ranked. Sourced from the spine via Loki (werk-verbs + platform-chorus jobs), computed OFF the serving loop (execs dist/flow-report-cli.js). Returns structured JSON; also refreshes ~/.chorus/reports/card-cycle-report.html (the standing form of the 06-06 one-off). The measure behind #3266\'s walk-away bar. Read-only. Output carries truncated=true if the Loki page cap was hit — never mistake a cap for completeness.',
  inputSchema: {
    type: 'object',
    properties: {
      hours: { type: 'integer', minimum: 1, maximum: 720, description: 'Window in hours. Default 120 (5 days).' },
    },
    additionalProperties: false,
  },
} as const;

// #2652 (AC8) — cards MCP tool defs. Each tool is a thin wrapper around the
// canonical cards bash CLI; MCP and bash callers run the same code path.
const CARDS_ADD_TOOL_DEF = {
  name: 'chorus_cards_add',
  description:
    'Create a new card on the team kanban board. Use this when filing tracked work — a feature, fix, follow-on, or formalized thread. Wraps the canonical `cards add` CLI; spawned with the calling role as DEPLOY_ROLE for attribution. Description must include Experience (what changes for the user) and AC (acceptance criteria, markdown checklist) so gate-product can read both. Do NOT use for ephemeral scratch notes or thoughts — those belong in role memory or a brief, not on the board.',
  inputSchema: {
    type: 'object',
    properties: {
      title: { type: 'string', minLength: 1, description: 'Short imperative card title' },
      owner: { type: 'string', enum: ['wren', 'silas', 'kade', 'jeff'], description: 'Owner role — pick one of these specific roles (wren=PM, silas=architect/ops, kade=engineer, jeff=human director)' },
      priority: { type: 'string', enum: ['P1', 'P2', 'P3'], description: 'Priority — pick one: P1=highest/now, P2=meaningful/soon, P3=eventual' },
      domain: { type: 'string', minLength: 1, description: 'Domain label (chorus, photos, seeds, ...)' },
      type: { type: 'string', enum: ['new', 'enhance', 'fix', 'chore', 'swat'], description: 'Card type — pick one: new=greenfield, enhance=existing-feature-improvement, fix=bug, chore=housekeeping, swat=crisis' },
      origin: { type: 'string', enum: ['reflective', 'reactive'], description: 'Origin — pick one: reflective=chosen work, reactive=responding to breakage' },
      desc: { type: 'string', minLength: 1, description: 'Markdown description (Experience + AC sections)' },
      sequence: { type: 'string', description: 'Sequence label — legacy axis (deprecated by subproduct per #2643)' },
      chunk: { type: 'string', description: 'Chunk label (app, ops, memory, ...)' },
      subproduct: { type: 'string', enum: ['athena', 'loom', 'werk', 'borg', 'convergence', 'clearing'], description: 'Subproduct — pick one Chorus implementation product: athena=ontology, loom=team-knowledge, werk=execution-substrate, borg=observability, convergence=integration, clearing=interaction. Refused if not in this closed list (#2652 AC2).' },
      subdomain: { type: 'string', description: 'Subdomain — Athena subdomain id (e.g. cards-service, gates-service). Refused if not in Athena (#2652 AC1).' },
    },
    required: ['title', 'owner', 'priority', 'domain', 'type', 'origin', 'desc'],
  },
} as const;

// #2996 — Jeff-initiated card add. Lives here (the agent-facing MCP server,
// :3341) because that is what .mcp.json's "chorus-api" entry actually targets.
const CARD_ADD_JEFF_TOOL_DEF = {
  name: 'chorus_card_add_jeff',
  description:
    'Use this ONLY when invoked by the `/card` skill on Jeff\'s direct request. Files a Jeff-attributed card by spawning `cards add` with DEPLOY_ROLE=jeff hardcoded — bouncer\'s isAgent check returns false, card lands directly with no approval-ask payload (it still needs the Experience+AC floor; #3293 removed --quick). The /card skill invocation IS the authorization. Do NOT use for agent-initiated cards (own observation, peer follow-on, demo seed) — use chorus_cards_add instead so the bouncer\'s six-section gate fires and Jeff sees the proposal. Do NOT call this to bypass a bouncer refusal on your own work; the bouncer is intentional. (#2996 retires the old natural-language detector + freshness-marker path in favor of this single typed tool.)',
  inputSchema: {
    type: 'object',
    properties: {
      title: { type: 'string', minLength: 1, description: 'Short imperative card title — what to do, not why' },
      owner: { type: 'string', enum: ['wren', 'silas', 'kade', 'jeff'], description: 'Owning role — pick one (wren=PM/loom, silas=ops/observe, kade=engineer/frontend, jeff=human)' },
      priority: { type: 'string', enum: ['P1', 'P2', 'P3'], description: 'Priority lane — pick one (P1=urgent/blocking, P2=soon, P3=eventual; default P3 if Jeff didn\'t signal urgency)' },
      domain: { type: 'string', minLength: 1, description: 'Domain label (e.g. chorus, gathering)' },
      type: { type: 'string', enum: ['new', 'enhance', 'fix', 'chore', 'swat'], description: 'Card type — pick one (new=feature, enhance=improve existing, fix=bug, chore=housekeeping, swat=crisis)' },
      origin: { type: 'string', enum: ['reflective', 'reactive'], description: 'Origin — pick one (reflective=chosen, reactive=in response to breakage)' },
      desc: { type: 'string', description: 'Optional description (Jeff-initiated cards skip the six-section gate)' },
      sequence: { type: 'string', description: 'Sequence label' },
      chunk: { type: 'string', description: 'Chunk label' },
      subproduct: { type: 'string', enum: ['athena', 'loom', 'werk', 'borg', 'convergence', 'clearing'], description: 'Subproduct — pick one (athena=KG, loom=principles, werk=execution, borg=monitoring, convergence=NiFi-pipeline, clearing=multi-role-chat)' },
      subdomain: { type: 'string', description: 'Subdomain — Athena subdomain id' },
    },
    required: ['title', 'owner', 'priority', 'domain', 'type', 'origin'],
  },
} as const;

const CARDS_MOVE_TOOL_DEF = {
  name: 'chorus_cards_move',
  description:
    'Move a card to a new status lane on the kanban board. Use this for routine board flow — Next→WIP when pulling, WIP→Blocked when stuck, Later→Next when triaged. status=Done is REFUSED here (enforced, not just discouraged): Done is owned by chorus_cards_done, the only verb that emits the card.accepted spine event subscribers depend on (DEC-048).',
  inputSchema: {
    type: 'object',
    properties: {
      id: { type: 'integer', minimum: 1, description: 'Card id' },
      status: { type: 'string', enum: ['Now', 'Next', 'Later', 'WIP', 'Blocked', 'Done', "Won't Do", 'Harvesting', 'SWAT'], description: 'New status — pick one of these specific lanes (Now=pulled, Next=queued, Later=parked, WIP=actively-building, Blocked=stuck, Done=accepted, Won\'t Do=killed, Harvesting=ingestion, SWAT=crisis)' },
    },
    required: ['id', 'status'],
  },
} as const;

const CARDS_DONE_TOOL_DEF = {
  name: 'chorus_cards_done',
  description:
    'Mark a card Done — the canonical acceptance verb. Use this when accepting completed work after demo + gate chain. Emits card.accepted spine event subscribers depend on. Do NOT use to self-accept code cards (DEC-048: builders cannot accept their own code work — Wren or Jeff invokes); do NOT use chorus_cards_move with status=Done as a substitute because that path skips the audit emit.',
  inputSchema: {
    type: 'object',
    properties: {
      id: { type: 'integer', minimum: 1, description: 'Card id to accept' },
    },
    required: ['id'],
  },
} as const;

const CARDS_TAG_TOOL_DEF = {
  name: 'chorus_cards_tag',
  description:
    'REMOVE a label-axis tag (sequence/domain/chunk) from a card — cards_tag is the removal verb only. SETTING a label value is owned by chorus_cards_set (ADR-031: one writer per field), so op=add is REFUSED here (enforced, not prose) with a pointer to cards_set. Use op=remove to clear a mis-tag during audits. Do NOT use for owner/priority/type/origin/title/status — structured fields go through chorus_cards_set.',
  inputSchema: {
    type: 'object',
    properties: {
      id: { type: 'integer', minimum: 1, description: 'Card id' },
      category: { type: 'string', enum: ['sequence', 'domain', 'chunk'], description: 'Tag axis — pick one: sequence=subproduct-tag, domain=top-level-area, chunk=abstract-category' },
      value: { type: 'string', minLength: 1, description: 'Tag value (e.g., werk, chorus, app)' },
      op: { type: 'string', enum: ['add', 'remove'], default: 'add', description: 'Operation — pick one: add (default) or remove' },
    },
    required: ['id', 'category', 'value'],
  },
} as const;

const CARDS_SET_TOOL_DEF = {
  name: 'chorus_cards_set',
  description:
    'The single writer for a card\'s descriptive properties — owner, priority, type, origin, title, subdomain, subproduct, and the label axes sequence/domain/chunk (typed validation applies: subproduct closed-list, subdomain must exist in Athena). Use it for any field change or multi-field update that should land together. Pass {fields: {priority: "P1", owner: "wren", sequence: "pulse"}}. STATUS is REFUSED here (enforced at the boundary, not prose) — status is a transition, not a field: use chorus_cards_move for non-Done lanes, chorus_cards_done for Done (emits card.accepted). Emits card.item.set per change.',
  inputSchema: {
    type: 'object',
    properties: {
      id: { type: 'integer', minimum: 1, description: 'Card id' },
      fields: {
        type: 'object',
        description: 'field=value map (priority, owner, title, status, type, origin, subdomain, subproduct, ...)',
        additionalProperties: { type: 'string' },
      },
    },
    required: ['id', 'fields'],
  },
} as const;

const CARDS_VIEW_TOOL_DEF = {
  name: 'chorus_cards_view',
  description:
    'Get full card detail as structured JSON — title, status, owner, priority, domains, description, comments. Use this when you need to inspect a card programmatically before acting on it (gates, demos, audits, conditional logic). Always invokes --json so the response shape is stable. Do NOT use this for human-readable output — use the bash `cards view <id>` directly which formats for terminal.',
  inputSchema: {
    type: 'object',
    properties: {
      id: { type: 'integer', minimum: 1, description: 'Card id to fetch' },
    },
    required: ['id'],
  },
} as const;

// #3268 — the priorities operating-layer readout, productized. Returns role →
// priority(chunk) → cards in hard-rank order, reading the board's CHUNK data
// (not the laggy search index, #3259). Read-side companion to the chunk-tagging.
const PRIORITIES_READOUT_TOOL_DEF = {
  name: 'chorus_priorities_readout',
  description:
    'Get the live priorities report: each role\'s chunks (priorities) → the cards under each (chunk → "- #id - title"), plus the cross-cut `proving` chunk. TAGGED-ONLY — a card with no chunk is not a priority and is not shown (no untagged, no prune). Reads the board (Vikunja) chunk data directly (not the search index), and renders ONE fixed deterministic format so the output is identical for every role and every call — it is a REPORT, not a recap. PRIMARY USE: pass `role` to get just that role\'s priorities (the usual "what are your priorities" answer). Omit `role` for the whole-team view (all three in hard rank). Note: in focus mode the caller must paste the returned text into its own reply — that text is the only thing the human sees.',
  inputSchema: {
    type: 'object',
    properties: {
      role: { type: 'string', enum: ['kade', 'wren', 'silas'], description: 'Return ONLY this role\'s priorities (the usual mode). Omit for the whole-team report.' },
    },
  },
} as const;

/** #3268 — a row as read from the board: card display-index, title, bucket, and
 *  its comma-joined label titles (owner:*, chunk:*, sequence:* …). */
export interface ReadoutRow { idx: number; title: string; bucket: string; labels: string | null; }
export interface ReadoutCard { id: number; title: string; bucket: string; owner: string; }

/** Hard rank Jeff set 2026-06-06: Kade(werk) 1 · Silas(model) 2 · Wren(loom+memory) 3. */
const READOUT_ROLE_RANK: Record<string, number> = { kade: 1, silas: 2, wren: 3 };

/** #3268 — PURE grouping (unit-tested): board rows → role→chunk→cards readout.
 *  - roles in hard-rank order; chunks = the funded priorities (from chunk:*).
 *  - `proving` is surfaced cross-cut (it spans roles).
 *  - `prune` = off-priority Gathering (sequence:gathering).
 *  - per-role `untagged` = chorus cards with no chunk → honest, never fabricated (AC4).
 *  No placement is invented: a card lands under a chunk ONLY if that chunk label is on it. */
export function groupPrioritiesReadout(rows: ReadoutRow[]) {
  const ROLES = ['kade', 'silas', 'wren'];
  type P = { id: number; title: string; bucket: string; owner: string; chunks: string[]; gathering: boolean };
  const parsed: P[] = rows.map((r) => {
    const labels = (r.labels || '').split(',').map((s) => s.trim()).filter(Boolean);
    let owner = 'unassigned';
    const chunks: string[] = [];
    let gathering = false;
    for (const l of labels) {
      if (l.startsWith('owner:')) owner = l.slice('owner:'.length);
      else if (l.startsWith('chunk:')) chunks.push(l.slice('chunk:'.length));
      else if (l === 'sequence:gathering') gathering = true;
    }
    return { id: r.idx, title: r.title, bucket: r.bucket, owner, chunks, gathering };
  });
  const toCard = (p: P): ReadoutCard => ({ id: p.id, title: p.title, bucket: p.bucket, owner: p.owner });
  const proving: ReadoutCard[] = [];
  const prune: ReadoutCard[] = [];
  const roleMap: Record<string, { chunks: Record<string, ReadoutCard[]>; untagged: ReadoutCard[] }> = {};
  for (const r of ROLES) roleMap[r] = { chunks: {}, untagged: [] };
  let chunked = 0;
  let untaggedN = 0;
  for (const p of parsed) {
    if (p.chunks.includes('proving')) proving.push(toCard(p));
    if (p.gathering) { prune.push(toCard(p)); continue; }
    if (!ROLES.includes(p.owner)) continue; // jeff/unassigned aren't in the hard rank
    const own = p.chunks.filter((c) => c !== 'proving');
    if (own.length === 0 && !p.chunks.includes('proving')) {
      roleMap[p.owner].untagged.push(toCard(p)); untaggedN++;
    } else {
      for (const c of own) { (roleMap[p.owner].chunks[c] ||= []).push(toCard(p)); chunked++; }
    }
  }
  const roles = ROLES.map((role) => ({
    role,
    rank: READOUT_ROLE_RANK[role],
    chunks: Object.keys(roleMap[role].chunks).sort().map((chunk) => ({ chunk, cards: roleMap[role].chunks[chunk] })),
    untagged: roleMap[role].untagged,
  })).sort((a, b) => a.rank - b.rank);
  return {
    roles,
    proving,
    prune,
    totals: { active: parsed.length, chunked, untagged: untaggedN, prune: prune.length },
  };
}

/** #3268 — THE canonical readable render (pure, unit-tested). ONE fixed shape, in code,
 *  so the format is never re-improvised per-role or per-call (Jeff, 2026-06-08: "i dont
 *  want to have to have a formatting discussion with each of u on this every time"):
 *
 *    N · ROLE
 *      <chunk> (n)
 *        - #id  title
 *      untagged (n)
 *    proving — cross-cut (n) / prune — Gathering (n)
 *
 *  This IS the tool's output for humans — not raw JSON, not a one-off swizzle. */
export function renderPrioritiesReadout(r: ReturnType<typeof groupPrioritiesReadout>): string {
  const card = (c: ReadoutCard) => `   - #${c.id} - ${c.title}`;
  const lines: string[] = [];
  for (const role of r.roles) {
    lines.push('');
    lines.push(`${role.rank} ${role.role.toUpperCase()}`);
    for (const ch of role.chunks) {
      lines.push(ch.chunk);
      for (const c of ch.cards) lines.push(card(c));
    }
    // #3268 — untagged cards are NOT shown (Jeff, 2026-06-08: "i dont want to see
    // things that are not tagged"). A priorities report shows tagged priorities only.
  }
  if (r.proving.length > 0) {
    lines.push('');
    lines.push('proving');
    for (const c of r.proving) lines.push(card(c));
  }
  // #3268 — no untagged, no prune. A priorities report shows ONLY chunked cards
  // (Jeff, 2026-06-08: "its clearly not a priority if it has no chunk"). proving is
  // included because chunk:proving IS a chunk (a cross-cut priority).
  return lines.join('\n').trimStart();
}

// #3268 — handler: read the board's chunk data straight from Vikunja (sqlite,
// what the cards CLI does internally) — NOT the search index — and hand the rows
// to the pure grouper. Active cards only (board view 8, excluding Done/Won't Do).
async function executePrioritiesReadout(
  execFileAsync: ExecFileAsync,
  roleFilter?: string,
): Promise<{ content: Array<{ type: 'text'; text: string }> }> {
  const home = process.env.HOME || '';
  const db = process.env.VIKUNJA_DB || `${home}/.chorus/vikunja/db/vikunja.db`;
  const sql =
    'SELECT t."index" AS idx, t.title AS title, b.title AS bucket, ' +
    '(SELECT group_concat(l.title) FROM label_tasks lt JOIN labels l ON l.id = lt.label_id WHERE lt.task_id = t.id) AS labels ' +
    'FROM tasks t ' +
    'JOIN task_buckets tb ON tb.task_id = t.id ' +
    'JOIN buckets b ON b.id = tb.bucket_id AND b.project_view_id = 8 ' +
    "WHERE b.title NOT IN ('Done', 'Won''t Do') " +
    'ORDER BY t."index"';
  let rows: ReadoutRow[]; // assigned in try; catch throws before any read
  try {
    const { stdout } = await execFileAsync('sqlite3', ['-json', db, sql], { timeout: 10_000 });
    rows = stdout.trim() ? (JSON.parse(stdout) as ReadoutRow[]) : [];
  } catch (err) {
    throw new Error(`priorities-readout: board read failed — ${(err as Error).message}`, { cause: err });
  }
  const readout = groupPrioritiesReadout(rows);
  // #3268 — role filter (PRIMARY mode): just that role's chunks + its own proving
  // cards. Omit role → whole-team report. Then render the ONE canonical format.
  const view = roleFilter
    ? {
        roles: readout.roles.filter((r) => r.role === roleFilter),
        proving: readout.proving.filter((c) => c.owner === roleFilter),
        prune: [] as ReadoutCard[],
        totals: readout.totals,
      }
    : readout;
  return {
    content: [{
      type: 'text' as const,
      text: renderPrioritiesReadout(view),
    }],
  };
}

function logEvent(level: 'info' | 'error', event: string, fields: Record<string, unknown>): void {
  process.stderr.write(JSON.stringify({ level, event, tool: 'chorus_nudge_message', ts: new Date().toISOString(), ...fields }) + '\n');
}

// #2661 — chorus_commit_status tool def. Read-only: agent gets back the role's
// active WIP card and the derived branch (`<role>/<card-id>`). Refuses with a
// typed reason if 0/2+ cards or board is unreachable. Refusal emits the same
// chorus_commit.status_queried spine event as success — silent return is the bug.
const COMMIT_STATUS_TOOL_DEF = {
  name: 'chorus_commit_status',
  description:
    'Get the role\'s current commit-state — active WIP card, derived branch (`<role>/<card-id>`), and refusal-readiness. Use before calling chorus_commit (write) to confirm the substrate sees the card you think you\'re building. Service derives card from the board (`status=WIP`, `owner=<role>`); agent never passes card_id. Refuses with `no-wip-card` (0 cards), `multi-wip` (>1), or `board-unreachable` (board API down). Do NOT use to fetch a specific card by id — that\'s chorus_cards_view.',
  inputSchema: {
    type: 'object',
    properties: {
      role: {
        type: 'string',
        enum: ['kade', 'wren', 'silas'],
        description: 'Role whose commit-state to query — kade, wren, or silas',
      },
    },
    required: ['role'],
    additionalProperties: false,
  },
} as const;

// #2661 — default board reader: GET /api/chorus/context/board/wip → filter by
// owner. The endpoint returns `{ data: { cards: [{ id, owner, title, ... }] } }`.
// Owner casing is capitalized server-side ("Kade"); we capitalize before filter.
// 5s fetch timeout — a slow/hung board surfaces as `board-unreachable` per AC3
// rather than hanging the caller.
const BOARD_FETCH_TIMEOUT_MS = 5_000;

function defaultBoardReader(fetchImpl: FetchImpl, apiBase: string): BoardReader {
  return async (role) => {
    const url = `${apiBase}/api/chorus/context/board/wip`;
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), BOARD_FETCH_TIMEOUT_MS);
    try {
      const resp = await fetchImpl(url, { signal: controller.signal } as never);
      if (!resp.ok) {
        return { ok: false, reason: 'board-unreachable', detail: `status ${resp.status ?? 'unknown'}` };
      }
      const body = (await resp.json()) as { data?: { cards?: BoardCard[] } };
      const allCards = body.data?.cards ?? [];
      const want = role.charAt(0).toUpperCase() + role.slice(1);
      const cards = allCards.filter((c) => c.owner === want);
      return { ok: true, cards };
    } catch (err) {
      const isAbort = err instanceof Error && (err.name === 'AbortError' || /aborted/i.test(err.message));
      const detail = isAbort
        ? `timeout after ${BOARD_FETCH_TIMEOUT_MS}ms`
        : err instanceof Error
          ? err.message
          : String(err);
      return { ok: false, reason: 'board-unreachable', detail };
    } finally {
      clearTimeout(timer);
    }
  };
}

// #2661 — default spine emitter: stderr JSON line. Aligns with logEvent shape.
// Production this is what chorus-api stderr → log file → spine-tail consumes.
function defaultSpineEmitter(): SpineEmitter {
  return (event, fields) => {
    process.stderr.write(JSON.stringify({ level: 'info', event, ts: new Date().toISOString(), ...fields }) + '\n');
  };
}

// #2845 + #2857 — trace + card scoped emitter factory. When traceId is provided,
// every emit from the returned closure carries trace_id in fields. When cardId
// is provided, every emit also carries card_id (per #2838 MUST-carry contract;
// callers omit cardId for system-event emitters so the field is structurally
// absent, not "card_id: null"). When both are omitted, returns the base emitter
// unchanged. Foundation for #2839/#2838 migration cohort: handlers mint at flow
// entry, construct an emitter once, propagate through called helpers.
export function createSpineEmitter(
  traceId?: string,
  base: SpineEmitter = defaultSpineEmitter(),
  cardId?: number,
): SpineEmitter {
  if (!traceId && !cardId) return base;
  return (event, fields) => {
    const merged: Record<string, unknown> = { ...fields };
    if (traceId) merged.trace_id = traceId;
    if (cardId) merged.card_id = cardId;
    return base(event, merged);
  };
}

// #2682 — chorus_commit (write) tool def. Wraps git-queue.sh commit + push
// behind one declarative call. Service derives card via boardReader; refuses
// with typed reasons (no-wip-card / multi-wip / board-unreachable from
// boardReader, branch-mismatch / hook-fail / push-conflict from git-queue
// exit + stderr classification).
const COMMIT_TOOL_DEF = {
  name: 'werk-commit',
  description:
    'Commit the card\'s werk changes. Thin skin over the rust werk-commit verb — MCP just execs it. Stages the card\'s ephemeral-werk changes (add -A; the werk IS the card\'s file set) and commits with message "<role>: #<card> — <summary>" through the pre-commit gate chain. Returns the sha. Do NOT use raw `git commit` or `bash git-queue.sh` (the retired v1 path). The /commit flow calls this and nothing else.',
  inputSchema: {
    type: 'object',
    properties: {
      role: { type: 'string', enum: ['kade', 'wren', 'silas'], description: 'Builder role owning the werk being committed.' },
      card_id: { type: 'integer', minimum: 1, description: 'Card ID whose werk changes to commit.' },
      summary: { type: 'string', minLength: 1, description: 'Optional short summary; message becomes "<role>: #<card> — <summary>".' },
    },
    required: ['role', 'card_id'],
    additionalProperties: false,
  },
} as const;

// #2759 — chorus_unpull_card tool def. /pull's atomic inverse.
const UNPULL_CARD_TOOL_DEF = {
  name: 'chorus_unpull_card',
  description:
    'DEPRECATED ALIAS (#3299, ADR-031): use werk-unpull — same contract, rust verb. Service runs validate (must be WIP + owned by role) + werk pre-flight (refuses werk-dirty) + cards move <id> Next + chorus-werk remove (removes the card\'s ephemeral worktree, deletes the branch, prunes stale admin entries, emits card.branch.closed) + role-state idle + card.unpulled spine event in one atomic transaction. Returns { role, card_id, prior_branch, branch_closed }. Refusal taxonomy: card-not-found | wrong-status | wrong-owner | werk-not-initialized | werk-dirty | move-fail | branch-close-fail. Do NOT use raw cards/git/role-state — those bypass the typed refusal taxonomy and leave stale branches. The /unpull skill calls this and nothing else.',
  inputSchema: {
    type: 'object',
    properties: {
      role: {
        type: 'string',
        enum: ['kade', 'wren', 'silas'],
        description: 'Calling role — kade / wren / silas. DEPLOY_ROLE attribution + spine role field.',
      },
      card_id: {
        type: 'integer',
        minimum: 1,
        description: 'Card ID to unpull. Must currently be in WIP status and owned by role.',
      },
    },
    required: ['role', 'card_id'],
    additionalProperties: false,
  },
} as const;

// #3299 — werk-unpull verb tool def: the rust port of chorus_unpull_card (the
// /pull inverse on the ADR-032 blueprint). chorus_unpull_card stays registered as
// a DEPRECATION ALIAS (ADR-031 rollout) delegating to the same verb binary.
const WERK_UNPULL_TOOL_DEF = {
  name: 'werk-unpull',
  description:
    'Use this to reverse a pull and tear down the role\'s WIP card cleanly. Thin skin over the rust werk-unpull verb — MCP just execs it. Runs validate (WIP + owned by role) + werk pre-flight (refuses werk-dirty so uncommitted work is never dropped) + cards move Next + chorus-werk remove + role-state idle + card.unpulled spine event; the two mutate steps are idempotent so a partial unpull re-runs to done. Returns the prior branch. Refusal taxonomy: card-not-found | wrong-status | wrong-owner | werk-not-initialized | werk-corrupt | werk-dirty | move-fail | branch-close-fail. The /unpull skill calls this and nothing else (chorus_unpull_card is the deprecated alias).',
  inputSchema: UNPULL_CARD_TOOL_DEF.inputSchema,
} as const;

// #3193 — werk-review verb tool def: the cold-eyes gate's binary half. floor /
// verdict / check as ONE tool (mode arg), thin skin over the rust verb.
const WERK_REVIEW_TOOL_DEF = {
  name: 'werk-review',
  description:
    'Use this for the cold-eyes review gate (#3193). Thin skin over the rust werk-review verb. Three modes: mode=floor runs the STRUCTURED FLOOR on the card\'s werk (merge-base diff; objective checks: unchecked AC, src-without-test, removed pub symbols w/ ast-grep survivor check) and records review.floor; mode=verdict records the cold-eyes agent\'s review.verdict (requires verdict pass|fail + findings — REJECTED if a fail has no findings or the floor never ran, the anti-ceremony guard); mode=check reads the latest verdict (pass→ok, fail-or-missing→error — the future hard-gate read). Advisory today: a fail informs Jeff\'s go. Refusal taxonomy: no-werk | no-ac | dirty-floor-inputs | ceremony-rejected. The /demo skill\'s cold-eyes subagent records through this.',
  inputSchema: {
    type: 'object',
    properties: {
      mode: { type: 'string', enum: ['floor', 'verdict', 'check'], description: 'floor = run objective checks; verdict = record the agent review; check = read the latest verdict.' },
      role: { type: 'string', enum: ['kade', 'wren', 'silas'], description: 'Builder role whose werk is reviewed (floor mode).' },
      card_id: { type: 'integer', minimum: 1, description: 'Card under review.' },
      verdict: { type: 'string', enum: ['pass', 'fail'], description: 'verdict mode only.' },
      findings: { type: 'string', description: 'verdict mode: specific findings (file:line / AC item N). Required on fail.' },
    },
    required: ['mode', 'card_id'],
    additionalProperties: false,
  },
} as const;

const PULL_CARD_TOOL_DEF = {
  name: 'werk-pull',
  description:
    'Use this to pull a card to WIP and ready the role\'s werk for building. Service runs validate + cards move WIP + chorus-werk add (creates the card\'s ephemeral worktree chorus-werk/<role>-<card>/ on branch <role>/<card-id>) + role-state building + card.pulled spine event in one atomic transaction. Returns { role, card_id, branch }. Refusal taxonomy: card-not-found | wrong-status | ac-missing | experience-missing | move-fail | branch-fail. Do NOT use raw cards/git/role-state — those bypass the typed refusal taxonomy. The /pull skill calls this and nothing else.',
  inputSchema: {
    type: 'object',
    properties: {
      role: {
        type: 'string',
        enum: ['kade', 'wren', 'silas'],
        description: 'Calling role — kade / wren / silas. DEPLOY_ROLE attribution + spine role field.',
      },
      card_id: {
        type: 'integer',
        minimum: 1,
        description: 'Card ID to pull. Must be in Next or Later status with AC + Experience populated.',
      },
    },
    required: ['role', 'card_id'],
    additionalProperties: false,
  },
} as const;

const LOOM_GEMBA_TOOL_DEF = {
  name: 'loom-gemba',
  description:
    'Observe a role working — one poll of the loom-gemba observation verb (#3319). Invoking it IS the declaration: the verb sets role-state `observing gemba=<target>`, gathers the target\'s fresh turns via the pulse-gather core (own durable cursor — exact, no replay, no loss), emits a gemba.observed spine event, and returns banner + turns. The FIRST stdout line is always the visibility banner `[gemba] <watcher>→<target> | since <cursor> | <n> new turns` — including empty polls (quiet) and missing-stream (rebuilding, never false-idle). In focus mode the caller MUST paste the returned text into its reply — that text is the only thing Jeff sees. Re-invoke to keep watching; there is no background loop. The /gemba skill calls this and nothing else.',
  inputSchema: {
    type: 'object',
    properties: {
      role: {
        type: 'string',
        enum: ['kade', 'wren', 'silas'],
        description: 'Observer role — who is watching.',
      },
      target: {
        type: 'string',
        enum: ['kade', 'wren', 'silas'],
        description: 'Target role being observed.',
      },
    },
    required: ['role', 'target'],
    additionalProperties: false,
  },
} as const;

const WERK_PUSH_TOOL_DEF = {
  name: 'werk-push',
  description:
    'Push the role\'s werk branch <role>/<card> to origin (rebases onto origin/main under the lock first). Thin skin over the rust werk-push verb — MCP just execs it. Returns the pushed sha. The /push flow calls this and nothing else.',
  inputSchema: {
    type: 'object',
    properties: {
      role: { type: 'string', enum: ['kade', 'wren', 'silas'], description: 'Builder role owning the werk being pushed.' },
      card_id: { type: 'integer', minimum: 1, description: 'Card ID whose werk branch to push.' },
    },
    required: ['role', 'card_id'],
    additionalProperties: false,
  },
} as const;

const WERK_MERGE_TOOL_DEF = {
  name: 'werk-merge',
  description:
    'Merge the role\'s pushed werk branch <role>/<card> to main. Thin skin over the rust werk-merge verb — MCP just execs it. Resolves the OPEN PR for the current HEAD oid (NOT the branch name — the #3175 fix for the stale-PR false-green), creates one if absent, squash-merges (the one merge mechanism), and CONTENT-VERIFIES the merge landed (PR MERGED + merge commit on origin/main) before returning the merged main sha. Refusal taxonomy: no-werk | branch-mismatch | no-open-pr | pr-create-fail | merge-conflict | not-mergeable | merge-fail. werk-accept is finalize-only and no longer merges (#3175). The /merge flow + werk-mcp.sh step 5 call this and nothing else.',
  inputSchema: {
    type: 'object',
    properties: {
      role: { type: 'string', enum: ['kade', 'wren', 'silas'], description: 'Builder role owning the werk branch being merged.' },
      card_id: { type: 'integer', minimum: 1, description: 'Card ID whose pushed branch to merge.' },
    },
    required: ['role', 'card_id'],
    additionalProperties: false,
  },
} as const;

const WERK_ACCEPT_TOOL_DEF = {
  name: 'werk-accept',
  description:
    'Finalize an accepted card: merge <role>/<card> to main, cards-done (emits card.accepted), close branch + werk. Thin skin over the rust werk-accept verb. `role` is the builder (werk location); the accepter is the calling identity — only jeff/wren may finalize (DEC-048). The /accept flow calls this and nothing else.',
  inputSchema: {
    type: 'object',
    properties: {
      role: { type: 'string', enum: ['kade', 'wren', 'silas'], description: 'Builder role whose card/werk is being accepted.' },
      card_id: { type: 'integer', minimum: 1, description: 'Card ID to finalize.' },
    },
    required: ['role', 'card_id'],
    additionalProperties: false,
  },
} as const;

const DESIGN_REFRESH_TOOL_DEF = {
  name: 'chorus_design_refresh',
  description:
    'Use this to refresh the cite-density sections (References, Path-to-close, Gaps) of a service-design HTML from current card statuses. Reads the design at designing/docs/<design_name>.html, validates template compliance (refuses if mandatory summary-block is absent), pulls Done/WIP/Next/Later/Won\'t-Do status for every #NNNN reference via the cards CLI, and regenerates the data-section-tagged headings without touching human-authored sections (Summary block, Overview, As-Is, To-Be, per-domain blocks). Auto-conforms docs that lack the canonical summary-block or data-section attrs (emits design.scaffold.inserted). Returns { design_name, sections_regenerated, cards_referenced, diff_lines, cards_by_status }. Refusal taxonomy: design-not-found | template-violation | summary-missing | manifest-missing | regenerate-fail. The /design-refresh skill calls this and nothing else. Do NOT use to author the substance of a design (Overview, As-Is, Domains) — that\'s human-authored; this only refreshes cite-density layers. Do NOT use on docs outside designing/docs/*-service-design.html — the template contract assumes that surface.',
  inputSchema: {
    type: 'object',
    properties: {
      role: {
        type: 'string',
        enum: ['kade', 'wren', 'silas'],
        description: 'Calling role — kade / wren / silas. DEPLOY_ROLE attribution + spine role field.',
      },
      design_name: {
        type: 'string',
        minLength: 1,
        description: 'Filename stem (or basename) of the service design HTML, e.g. "build-and-deploy-service-design". Looked up under designing/docs/<name>.html.',
      },
    },
    required: ['role', 'design_name'],
    additionalProperties: false,
  },
} as const;

const DOC_CATALOG_ADD_TOOL_DEF = {
  name: 'chorus_doc_catalog_add',
  description:
    'Register a doc (.html or .md) in the Chorus doc-catalog so it surfaces in the Athena UI. Use this when a new design doc, ADR, or service-design lands and should be discoverable team-wide. Required: filePath (absolute, must exist) + href (public path the UI uses, must be unique). Optional: group (free-text cluster label). Returns the registered entry on success. Refusals (each maps to an HTTP status from registerDoc): invalid-input (missing filePath/href, status 400) | file-not-found (path does not exist, 404) | invalid-extension (not .html or .md, 400) | already-registered (href collision, 409). Do NOT use to update an existing entry — there is no update path; remove + re-add via the same flow if needed. Do NOT use to bulk-import without checking duplicates first — already-registered is per-href and will refuse 1 entry at a time. The HTTP endpoint POST /api/doc-catalog/add is the same code path; agents should prefer this MCP tool over bash-curl from a role session.',
  inputSchema: {
    type: 'object',
    properties: {
      filePath: {
        type: 'string',
        minLength: 1,
        description: 'Absolute path to the .html or .md file to register. Must exist on disk.',
      },
      href: {
        type: 'string',
        minLength: 1,
        description: 'Public href the Athena UI uses to reach the doc (e.g., "/gathering-docs/foo.html"). Must be unique.',
      },
      group: {
        type: 'string',
        description: 'Optional logical grouping label for UI clustering.',
      },
    },
    required: ['filePath', 'href'],
    additionalProperties: false,
  },
} as const;

// #2840 — typed agent surface for log + error investigation. Earns its keep
// on top of #2857's trace_id + card_id propagation: agents query by id and
// the substrate returns the full flow as structured rows, not blobs.
const TimeWindowEnum = z.enum(['5m', '15m', '1h', '6h', '1d']);

const LogsQueryInput = z.object({
  query: z.string().min(1).describe('LogQL query, e.g. {job="chorus-api"} |~ "chorus_acp"'),
  start: z.string().optional().describe('ISO 8601 timestamp; default: 1h ago'),
  end: z.string().optional().describe('ISO 8601 timestamp; default: now'),
  time_window: TimeWindowEnum.optional().describe('Convenience window (overrides start/end if both unset). Default 1h.'),
  limit: z.number().int().min(1).max(1000).optional().describe('Max events. Default 100, max 1000.'),
});

const LogsRecentErrorsInput = z.object({
  role: z.string().optional().describe('Filter to events from one role. Omit for all roles.'),
  time_window: TimeWindowEnum.optional().describe('Window. Default 1h.'),
});

const LogsForCardInput = z.object({
  card_id: z.number().int().min(1).describe('Card id to query. Returns every event whose payload contains this card_id (#2838).'),
  time_window: TimeWindowEnum.optional().describe('Window. Default 1d.'),
});

const LogsForTraceInput = z.object({
  trace_id: z.string().min(1).describe('Transaction-scope trace_id (UUIDv7). Returns every event of that flow (#2839).'),
  time_window: TimeWindowEnum.optional().describe('Window. Default 1h.'),
});

const LogsForBranchInput = z.object({
  branch: z.string().min(1).describe('Git branch the work ran on, e.g. kade/3023. Returns every event stamped with this branch (#3023).'),
  time_window: TimeWindowEnum.optional().describe('Window. Default 1d.'),
});

// #3029 — pain board MCP surface. The rollup logic lives once, in chorus-api
// (handlers/logs-query.queryPainRollup). These tools PROXY the existing
// /api/chorus/pain/* endpoints — no second copy, same numbers the page shows.
const PainRollupInput = z.object({
  window: z.string().optional().describe('Rollup window — e.g. 12h, 1d, 7d, 30d. Default 7d. chorus-api is the single validator.'),
});

const PainCardInput = z.object({
  card_id: z.number().int().min(1).describe('Card id — returns its pipeline-run trace (events grouped by trace_id).'),
});

const TIME_WINDOW_DESC = 'Time range for the query — pick one: 5m=five minutes, 15m=fifteen minutes, 1h=one hour, 6h=six hours, 1d=one day. Larger windows scan more Loki data.';

const LOGS_QUERY_TOOL_DEF = {
  name: 'chorus_logs_query',
  description:
    'Use this to run a custom LogQL query against Chorus logs in Loki when none of the convenience tools fit. Returns structured rows ({ events, count, truncated }) instead of blobs. Refusal taxonomy: loki-unreachable | query-syntax-error | time-range-invalid | result-too-large | rate-limited. Do NOT use for the common cases — chorus_logs_for_trace / chorus_logs_for_card / chorus_logs_recent_errors are tighter typed wrappers; reach for this only when you need a custom LogQL filter.',
  inputSchema: {
    type: 'object',
    properties: {
      query: { type: 'string', description: 'LogQL string, e.g. {job="chorus-api"} |~ "card.demo.started"' },
      start: { type: 'string', description: 'ISO 8601 timestamp; default 1h ago' },
      end: { type: 'string', description: 'ISO 8601 timestamp; default now' },
      time_window: { type: 'string', enum: ['5m', '15m', '1h', '6h', '1d'], description: TIME_WINDOW_DESC + ' Default 1h.' },
      limit: { type: 'integer', minimum: 1, maximum: 1000, description: 'Max events. Default 100.' },
    },
    required: ['query'],
    additionalProperties: false,
  },
} as const;

const LOGS_RECENT_ERRORS_TOOL_DEF = {
  name: 'chorus_logs_recent_errors',
  description:
    'Use this to answer "what broke recently?" — returns recent error-level events across the spine, optionally filtered to one role. Default window 1h. Do NOT use to investigate a known card or trace_id (use chorus_logs_for_card or chorus_logs_for_trace) or to grep for specific event names (use chorus_logs_query). Same refusal taxonomy as chorus_logs_query.',
  inputSchema: {
    type: 'object',
    properties: {
      role: { type: 'string', description: 'Filter to one role (kade / wren / silas / system). Omit for all.' },
      time_window: { type: 'string', enum: ['5m', '15m', '1h', '6h', '1d'], description: TIME_WINDOW_DESC + ' Default 1h.' },
    },
    additionalProperties: false,
  },
} as const;

const LOGS_FOR_CARD_TOOL_DEF = {
  name: 'chorus_logs_for_card',
  description:
    'Use this to retrieve every event bound to one card_id — gate emits, demo events, /acp step events, hook bites, anything that happened during work on card #N. Backed by #2838 card_id propagation. Default window 1d. Do NOT use for system events (heartbeats, health probes, canonical sync) — those are not card-bound and won t appear; for those use chorus_logs_query or chorus_logs_recent_errors.',
  inputSchema: {
    type: 'object',
    properties: {
      card_id: { type: 'integer', minimum: 1, description: 'Card id' },
      time_window: { type: 'string', enum: ['5m', '15m', '1h', '6h', '1d'], description: TIME_WINDOW_DESC + ' Default 1d.' },
    },
    required: ['card_id'],
    additionalProperties: false,
  },
} as const;

const LOGS_FOR_TRACE_TOOL_DEF = {
  name: 'chorus_logs_for_trace',
  description:
    'Use this to retrieve every event of one transaction end-to-end (one /acp, one chorus_pull_card, one build cycle) by trace_id. Returns the full causal chain as structured rows. Backed by #2839 trace_id propagation. Default window 1h. Do NOT use to find work-bound events (use chorus_logs_for_card) or to grep recent errors broadly (use chorus_logs_recent_errors).',
  inputSchema: {
    type: 'object',
    properties: {
      trace_id: { type: 'string', description: 'UUIDv7 trace_id (from MCP response or chorus_logs_query result)' },
      time_window: { type: 'string', enum: ['5m', '15m', '1h', '6h', '1d'], description: TIME_WINDOW_DESC + ' Default 1h.' },
    },
    required: ['trace_id'],
    additionalProperties: false,
  },
} as const;

const LOGS_FOR_BRANCH_TOOL_DEF = {
  name: 'chorus_logs_for_branch',
  description:
    'Use this to retrieve every event stamped with one git branch (e.g. kade/3023) — the git surface the work actually ran on. The third observability key: card_id = the whole chain, trace_id = one action, branch = where it ran. Backed by #3023 branch propagation. Default window 1d. Use it to catch card-vs-werk divergence (a step that ran on the wrong werk shows the wrong branch). Do NOT use for the card chain (use chorus_logs_for_card) or a single action (use chorus_logs_for_trace).',
  inputSchema: {
    type: 'object',
    properties: {
      branch: { type: 'string', description: 'Git branch, e.g. kade/3023' },
      time_window: { type: 'string', enum: ['5m', '15m', '1h', '6h', '1d'], description: TIME_WINDOW_DESC + ' Default 1d.' },
    },
    required: ['branch'],
    additionalProperties: false,
  },
} as const;

const PAIN_ROLLUP_TOOL_DEF = {
  name: 'chorus_pain_rollup',
  description:
    'Use this to see the team\'s pain in aggregate — spine failures grouped by class (role · event · reason), ranked by impact, split per product (Chorus / Gathering), with the cards / latest / sample-detail for each class. This is the in-session surface for the #3029 pain board (the browser page /borg/pain.html shows the same numbers). Reach for it to answer "what is hurting us most right now?" before pulling fix work. Do NOT use to investigate one known card (use chorus_pain_card) or to grep raw events (use chorus_logs_query).',
  inputSchema: {
    type: 'object',
    properties: {
      window: { type: 'string', description: 'Rollup window — e.g. 12h, 1d, 7d, 30d. Default 7d.' },
    },
    additionalProperties: false,
  },
} as const;

const PAIN_CARD_TOOL_DEF = {
  name: 'chorus_pain_card',
  description:
    'Use this to see one card\'s pipeline runs — the card broken into trace-keyed runs (pull / commit / acp / build), each pass/fail with steps + failure reason. The per-card view of the #3029 pain board. Do NOT use for the aggregate failure picture (use chorus_pain_rollup) or for arbitrary event greps (use chorus_logs_for_card / chorus_logs_query).',
  inputSchema: {
    type: 'object',
    properties: {
      card_id: { type: 'integer', minimum: 1, description: 'Card id' },
    },
    required: ['card_id'],
    additionalProperties: false,
  },
} as const;

// #2940 — Athena Move 0 tree-query MCP tools.
// data/athena/tree.json is the single source-of-truth for the structural model.
// These three tools surface it for /demo, /gate-product, role-nudge routing.

const TreeGetInput = z.object({});

const OwnershipLookupInput = z.object({
  iri: z
    .string()
    .regex(/^chorus:[a-z0-9][a-z0-9-]*$/i, 'IRI must match chorus:<slug>')
    .describe('IRI to look up — e.g., chorus:athena, chorus:cards-service, chorus:cards'),
});

const BlastRadiusInput = z.object({
  iri: z
    .string()
    .regex(/^chorus:[a-z0-9][a-z0-9-]*$/i, 'IRI must match chorus:<slug>')
    .describe('IRI whose inferred consumers + dependents you want — Product, Domain, or Service'),
});

const TREE_GET_TOOL_DEF = {
  name: 'chorus_tree_get',
  description:
    'Use this to fetch the full Athena structural tree — every Product, Domain, Service with their stored attributes (label, comment, vision, audience, status, gaps, ownership, value-stream-step, design-doc edges) and containment edges (hasChild, hasDomain, hosts, contains, consumes). Source: data/athena/tree.json (Athena Move 0 hand-authored; Move 5 ingests this into Fuseki via composite-POST). Use when you need the whole tree at once, e.g. for the rendered tree.html page or a session-start envelope. Do NOT call repeatedly per IRI — use chorus_ownership_lookup for single-IRI lookups (cheaper). Refusal taxonomy: tree-not-found | schema-violation (tree.json failed Zod parse).',
  inputSchema: { type: 'object', properties: {}, required: [], additionalProperties: false },
} as const;

const OWNERSHIP_LOOKUP_TOOL_DEF = {
  name: 'chorus_ownership_lookup',
  description:
    'Use this to answer "who owns this IRI and where does it sit in the tree?" Returns { iri, kind: product|domain|service, owner: role-iri, product, domain, service } — the containing-path filled in for the IRI\'s kind. Source: data/athena/tree.json. Use for role-nudge routing (resolve the IRI to a role before nudging), /gate-product domain-existence checks, demo product-impact framing. Do NOT use to fetch the IRI\'s full attribute set — use chorus_tree_get for that. Refusal taxonomy: not-found (IRI not in tree) | schema-violation.',
  inputSchema: {
    type: 'object',
    properties: {
      iri: {
        type: 'string',
        pattern: '^chorus:[a-z0-9][a-z0-9-]*$',
        description: 'IRI to look up — e.g., chorus:athena, chorus:cards-service, chorus:cards',
      },
    },
    required: ['iri'],
    additionalProperties: false,
  },
} as const;

const BLAST_RADIUS_TOOL_DEF = {
  name: 'chorus_blast_radius',
  description:
    'Use this to compute the inferred blast-radius of an IRI — who is affected if this Product/Domain/Service changes or breaks? Returns { iri, consumers: [iris], dependents: [iris], hosts?: [iris] }. Inference walks the structural graph: for a Service, consumers = Products consuming via chorus:consumes. For a Domain, consumers = Products with hasDomain → this Domain + (recursively) consumers of the Domain\'s hosted Services. For a Product, consumers = parent Products + (recursively) consumers of its Domains. Source: data/athena/tree.json. Use for /demo product-impact answers, /gate-product cross-domain risk surfacing, post-incident reconstruction. Do NOT use as a replacement for human judgment — the graph names structural blast-radius, not semantic impact. Refusal taxonomy: not-found (IRI not in tree) | schema-violation.',
  inputSchema: {
    type: 'object',
    properties: {
      iri: {
        type: 'string',
        pattern: '^chorus:[a-z0-9][a-z0-9-]*$',
        description: 'IRI whose blast-radius you want — Product / Domain / Service.',
      },
    },
    required: ['iri'],
    additionalProperties: false,
  },
} as const;

// Script / directory name used in path.join across pull, acp, and unpull flows.
const CHORUS_WERK = 'chorus-werk';
// Spine event emitted by all three athena query tools.
const EVT_ATHENA_TREE_QUERIED = 'athena.tree.queried';


// #3331 — classifyCommitFailure / prCreateMeansAlreadyMerged / commitFailureDetail /
// findMissingPaths REMOVED: orphaned exports of the cut v1 chorus_commit / chorus_acp
// paths (#3178 retired chorus_commit; the live werk-commit path goes through
// executeWerkVerb's generic reason= parse). Zero call sites confirmed semantically
// (ast-grep across the repo: no calls; only the dead test files #3324 already
// deleted referenced them).


// #2913 — ephemeral-worktree resolver. Replaces the #2750 CHORUS_WERK_ENABLE
// flag-router. Under the ephemeral model (chorus-werk/<role>-<card>/) there is
// no per-role opt-in flag and no persistent per-role werk to route to: a role's
// working tree IS the card's worktree, which exists only between /pull and /acp.
//
// Resolution — single-card glob (#2913 path (a)):
//   - exactly one chorus-werk/<role>-* dir → that is the role's active werk
//   - zero → no card in flight; fall back to canonical (same as old flag-off)
//   - more than one → ambiguous; the >1-card case needs an explicit card_id
//     from the caller. That refinement is #2920 (MCP-affordance multi-card).
//     Until then, return canonical so a wrong single werk is never guessed.
//
// No caching (the #2779 lesson): the set of werk dirs changes within a session
// as cards are pulled and acp'd. readdir is microseconds; this is per-MCP-
// request. Correctness over a cache that silently goes stale.
// #3331 — resolveWerkBinDir REMOVED: the acp promote-werk-bin path that consumed it
// is gone (promote = rebuild + prove-cdhash, not copy). Zero call sites confirmed
// semantically (ast-grep: no calls anywhere).

export function defaultResolveWorkingTree(canonicalRoot: string): (role: 'kade' | 'wren' | 'silas') => string {
   
  const fs = require('node:fs') as typeof import('node:fs');
   
  const path = require('node:path') as typeof import('node:path');

  return (role: 'kade' | 'wren' | 'silas'): string => {
    // CHORUS_WERK_BASE convention: sibling of canonical, parent dir + /chorus-werk/
    const werkBase = path.join(path.dirname(canonicalRoot), CHORUS_WERK);
    let matches: string[]; // assigned in try; catch returns before any read
    try {
      matches = fs.readdirSync(werkBase, { withFileTypes: true })
        // #3038: card werks only — `<role>-<digits>`. The per-role binary slot
        // `<role>-bin` (chorus-env-setup.sh) shares this namespace; counting it
        // returned the bin slot (1 match at pull → branch-fail) or canonical
        // (2 matches at commit → "On branch main"). Numeric-suffix match excludes
        // `-bin` and any other non-card slot.
        .filter((e) => e.isDirectory() && e.name.startsWith(`${role}-`) && /^\d+$/.test(e.name.slice(role.length + 1)))
        .map((e) => path.join(werkBase, e.name));
    } catch {
      // werk base missing/unreadable — no werk in flight, fall back to canonical
      return canonicalRoot;
    }
    if (matches.length === 1) {
      return matches[0];
    }
    // zero or ambiguous-multiple — fall back to canonical rather than guess
    return canonicalRoot;
  };
}

// #3182 — executeCommit deleted: dead since #3178 repointed chorus_commit→werk-commit
// (the rust verb). The git-queue.sh commit/push it wrapped is now werk-commit/werk-push.


// #3299 — executeUnpullCard (the #2759 TS implementation) REMOVED: the logic now
// lives in the rust werk-unpull verb (ADR-032 blueprint); both the werk-unpull
// tool and the chorus_unpull_card deprecation alias exec the binary via
// executeWerkVerb. Zero call sites confirmed (ast-grep) after the dispatch swap.

async function executeCommitStatus(
  args: { role: 'kade' | 'wren' | 'silas' },
  boardReader: BoardReader,
  emit: SpineEmitter,
): Promise<{ content: Array<{ type: 'text'; text: string }> }> {
  const { role } = args;
  const board = await boardReader(role);

  if (!board.ok) {
    emit('chorus_commit.status_queried', { role, reason: board.reason, detail: board.detail });
    throw new Error(`chorus_commit_status refused: board-unreachable${board.detail ? ` (${board.detail})` : ''}`);
  }

  if (board.cards.length === 0) {
    emit('chorus_commit.status_queried', { role, reason: 'no-wip-card' });
    throw new Error(`chorus_commit_status refused: no-wip-card — role ${role} has no card in WIP`);
  }

  if (board.cards.length > 1) {
    const ids = board.cards.map((c) => c.id).join(',');
    emit('chorus_commit.status_queried', { role, reason: 'multi-wip', card_ids: ids });
    throw new Error(`chorus_commit_status refused: multi-wip — role ${role} has ${board.cards.length} cards in WIP (${ids})`);
  }

  const card = board.cards[0];
  const branch = `${role}/${card.id}`;
  emit('chorus_commit.status_queried', { role, card_id: card.id });
  return {
    content: [
      {
        type: 'text',
        text: JSON.stringify({ role, card_id: card.id, branch, title: card.title }, null, 2),
      },
    ],
  };
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

// #3010 — return structuredContent.principles alongside the existing prose
// text. chorus-hooks consumes the JSON array directly (no client-side
// parse_tool_text rfind('(') greedy-match against prose, which fragmented
// principles whose comments contain parens — e.g. the Hemenway
// catch-and-store principle's "(in slope, charge, temperature, or otherwise)"
// captured the comment fragment as the principle id and dropped real
// principles from the boot envelope).
//
// Both content and structuredContent are populated for one rollout window:
// the text path stays so older clients keep working until they're updated;
// new clients prefer structuredContent. parse_tool_text retired as a
// follow-on once chorus-mcp is fully shipped and clients no longer fall
// back to the text path.
async function executePrinciplesList(
  fetchImpl: FetchImpl,
  apiBase: string,
  from: string,
): Promise<{
  content: Array<{ type: 'text'; text: string }>;
  structuredContent: { principles: PrincipleRecord[] };
}> {
  process.stderr.write(JSON.stringify({ level: 'info', event: 'mcp.principles.list.invoked', tool: 'chorus_principles_list', from, ts: new Date().toISOString() }) + '\n');
  const principles = await fetchPrinciplesList(fetchImpl, apiBase);
  const lines: string[] = [`${principles.length} principle${principles.length === 1 ? '' : 's'}:`];
  for (const p of principles) {
    const label = p.label ? `${p.label} (${p.id})` : p.id;
    const summary = p.comment ? `${label} — ${p.comment}` : label;
    lines.push(`- ${summary}`);
  }
  return {
    content: [{ type: 'text', text: lines.join('\n') }],
    structuredContent: { principles },
  };
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

// #2969 — chorus_doc_catalog_add: POST to /api/doc-catalog/add, which delegates
// to registerDoc() in handlers/doc-catalog.ts. Same code path the HTTP endpoint
// uses; this is the typed agent-facing surface. Refusal taxonomy mirrors
// registerDoc's HTTP status codes: 400/404/409.
// cog-override: registerDoc 400/404/409 outcome branches — pre-existing MCP-monolith complexity, not in #3173 (promote-slot fix) scope
async function executeDocCatalogAdd(
  args: { filePath: string; href: string; group?: string },
  fetchImpl: FetchImpl,
  apiBase: string,
  from: string,
): Promise<{ content: Array<{ type: 'text'; text: string }> }> {
  process.stderr.write(JSON.stringify({ level: 'info', event: 'mcp.doc_catalog.add.invoked', tool: 'chorus_doc_catalog_add', from, href: args.href, ts: new Date().toISOString() }) + '\n');
  const url = `${apiBase}/api/doc-catalog/add`;
  const body: Record<string, string> = { filePath: args.filePath, href: args.href };
  if (args.group) body.group = args.group;
  const resp = await fetchImpl(url, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  if (!resp.ok) {
    const status = resp.status ?? 0;
    let errMsg = `doc-catalog add failed (status ${status})`;
    try {
      const errBody = (await resp.json()) as { error?: string };
      if (errBody.error) errMsg = `${errMsg}: ${errBody.error}`;
    } catch {
      // body wasn't JSON; keep the status-only message
    }
    if (status === 400) throw new Error(`invalid-input — ${errMsg}`);
    if (status === 404) throw new Error(`file-not-found — ${errMsg}`);
    if (status === 409) throw new Error(`already-registered — ${errMsg}`);
    throw new Error(errMsg);
  }
  const result = (await resp.json()) as { registered?: { filePath: string; href: string; group?: string } };
  const reg = result.registered;
  const summary = reg
    ? `registered: ${reg.href} → ${reg.filePath}${reg.group ? ` [group: ${reg.group}]` : ''}`
    : 'registered (response missing registered field)';
  return { content: [{ type: 'text', text: summary }] };
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

interface SubdomainRecord {
  id: string;
  label?: string;
  owner?: string;
  step?: string;
  uri?: string;
}

async function fetchSubdomainsList(fetchImpl: FetchImpl, apiBase: string): Promise<SubdomainRecord[]> {
  const url = `${apiBase}/api/athena/subdomains`;
  const resp = await fetchImpl(url);
  if (!resp.ok) {
    throw new Error(`subdomains list fetch failed (status ${resp.status ?? 'unknown'})`);
  }
  const body = (await resp.json()) as { data?: SubdomainRecord[] };
  return body.data ?? [];
}

// eslint-disable-next-line @typescript-eslint/no-unused-vars -- parked subdomains executor, intentionally retained for future wiring (#3429)
async function executeSubdomainsList(
  fetchImpl: FetchImpl,
  apiBase: string,
  from: string,
): Promise<{ content: Array<{ type: 'text'; text: string }> }> {
  process.stderr.write(JSON.stringify({ level: 'info', event: 'mcp.subdomains.list.invoked', tool: 'chorus_subdomains_list', from, ts: new Date().toISOString() }) + '\n');
  const subs = await fetchSubdomainsList(fetchImpl, apiBase);
  const lines: string[] = [`${subs.length} subdomain${subs.length === 1 ? '' : 's'}:`];
  for (const s of subs) {
    const label = s.label ? `${s.label} (${s.id})` : s.id;
    const ownerStep = [s.owner ? `owner=${s.owner}` : '', s.step ? `step=${s.step}` : '']
      .filter(Boolean)
      .join(' ');
    lines.push(ownerStep ? `- ${label} — ${ownerStep}` : `- ${label}`);
  }
  return { content: [{ type: 'text', text: lines.join('\n') }] };
}

// eslint-disable-next-line @typescript-eslint/no-unused-vars -- parked subdomains executor, intentionally retained for future wiring (#3429)
async function executeSubdomainsGet(
  args: { id: string },
  fetchImpl: FetchImpl,
  apiBase: string,
  from: string,
): Promise<{ content: Array<{ type: 'text'; text: string }> }> {
  // Athena has no GET /api/athena/subdomains/:id today — fetch the list and
  // filter, mirroring the principles/decisions pattern. ~50 entries, fine
  // at this scale; swap to dedicated GET when one lands.
  process.stderr.write(JSON.stringify({ level: 'info', event: 'mcp.subdomains.get.invoked', tool: 'chorus_subdomains_get', from, id: args.id, ts: new Date().toISOString() }) + '\n');
  const subs = await fetchSubdomainsList(fetchImpl, apiBase);
  const found = subs.find((s) => s.id === args.id);
  if (!found) throw new Error(`subdomain not found: ${args.id}`);
  const lines = [`${found.label ?? found.id} (${found.id})`];
  if (found.owner) lines.push(`owner: ${found.owner}`);
  if (found.step) lines.push(`step: ${found.step}`);
  if (found.uri) lines.push(`uri: ${found.uri}`);
  return { content: [{ type: 'text', text: lines.join('\n') }] };
}

// #2804 — UUIDv7 trace_id for MCP-originated nudges. Hand-rolled (no new
// dep) per RFC 9562 §5.7: 48-bit Unix-ms timestamp + version + 12-bit
// random + variant + 62-bit random. Matches the format chorus-hooks/Rust
// side mints in #2765 so trace_ids are uniform across senders.
function mintTraceIdV7(): string {
  const { randomBytes } = require('crypto') as typeof import('crypto');
  const tsMs = BigInt(Date.now());
  const randBuf = randomBytes(10);
  const randHi = randBuf[0] & 0x0f;
  // Set variant (RFC 4122) — top two bits of byte 2 → 10
  randBuf[2] = (randBuf[2] & 0x3f) | 0x80;
  const tsHex = tsMs.toString(16).padStart(12, '0');
  const verRand = (0x7000 | (randHi << 8) | randBuf[1]).toString(16).padStart(4, '0');
  const lowHex = Array.from(randBuf.subarray(2)).map(b => b.toString(16).padStart(2, '0')).join('');
  return `${tsHex.slice(0, 8)}-${tsHex.slice(8, 12)}-${verRand}-${lowHex.slice(0, 4)}-${lowHex.slice(4, 16)}`;
}

// #2804 — append a chorus.log entry from the MCP server. Format matches
// the canonical chorus.log line shape (timestamp + event + role + fields).
async function appendChorusLog(event: string, role: string, fields: Record<string, unknown>): Promise<void> {
  const home = process.env.HOME || '/Users/jeffbridwell';
  const logPath = process.env.CHORUS_LOG_FILE || `${home}/.chorus/chorus.log`;
  const line = JSON.stringify({
    timestamp: new Date().toISOString(),
    event,
    role,
    ...fields,
  }) + '\n';
  try {
    const fs = await import('fs/promises');
    await fs.appendFile(logPath, line);
  } catch (err) {
    // best-effort spine write; do not block delivery
    logEvent('error', 'mcp.nudge.spine_write_failed', { error: String(err) });
  }
  // #3001 — push mcp.*.error events to silas's terminal via the existing
  // nudge persistence path. Ops alerts go to the ops role, not to Jeff.
  // Best-effort: pulse POST failure is logged but doesn't cascade.
  if (event === 'mcp.tool.error' || event === 'mcp.transport.error' || event === 'mcp.process.error') {
    void notifySilasOfMcpError(event, fields);
  }
}

// #3335 — synthetic/test card-id sentinels. MIRRORS the canonical predicate in
// chorus-api logs-query.ts (SYNTHETIC_CARD_IDS, #3029) — kept in sync by value because
// the two live in separate packages with no shared module today. UNIFYING these into a
// shared package (chorus-sdk) is the proper one-path fix; carded as a follow-on.
export const SYNTHETIC_CARD_IDS = new Set(['99998', '99999']);

/** #3335 — decide whether an mcp.tool.error should NUDGE ops (silas). Pure + testable.
 *  Suppress the NUDGE only — the spine event was already written by the caller, so a
 *  suppressed nudge stays fully observable (#3283: never add a loss mechanism). False when:
 *  - synthetic === true: the caller ran with CHORUS_SYNTHETIC=1 (a test harness running the
 *    server in-process, e.g. #3329's MCP-wrapper tests) — its errors are not ops incidents.
 *  - cardId is a synthetic sentinel (99998/99999).
 *  - errorMessage is a caller-side validation/refusal (#3022) — anchored as ONE group so a
 *    real error merely CONTAINING "expected one of"/"refused:" mid-message is NOT suppressed
 *    (Pattern-9 over-suppression fix: the old alternation left those branches unanchored). */
export function shouldNotifyOps(errorMessage: string, cardId: string, synthetic: boolean): boolean {
  if (synthetic) return false;
  if (SYNTHETIC_CARD_IDS.has(cardId)) return false;
  if (/^(Invalid (arguments|option)|expected one of|refused:)/i.test(errorMessage)) return false;
  return true;
}

// #3429 — safe stringify for unknown field values (no [object Object] from
// String() coercion; satisfies @typescript-eslint/no-base-to-string).
function fieldStr(v: unknown): string {
  if (v === null || v === undefined) return '';
  if (typeof v === 'string') return v;
  if (typeof v === 'number' || typeof v === 'boolean') return String(v);
  return JSON.stringify(v);
}

async function notifySilasOfMcpError(event: string, fields: Record<string, unknown>): Promise<void> {
  const tool = fieldStr(fields['tool']);
  const errorType = fieldStr(fields['error_type'] ?? fields['kind']);
  const errorMessage = fieldStr(fields['error_message']);
  const traceId = fieldStr(fields['trace_id']);
  const cardId = fieldStr(fields['card_id'] ?? fields['card']);
  // #3022 + #3335: only unexpected/systemic failures notify ops. Caller-side validation/
  // refusals (their own bad call, already returned to them) AND synthetic/test traffic
  // (CHORUS_SYNTHETIC=1 or a synthetic card-id) are suppressed from the NUDGE — the spine
  // event is already written above, so they stay fully observable. Suppress nudge, not event.
  if (!shouldNotifyOps(errorMessage, cardId, process.env.CHORUS_SYNTHETIC === '1')) {
    return;
  }
  const summary = [
    '[mcp.error]',
    event,
    tool && `tool=${tool}`,
    errorType && `type=${errorType}`,
    errorMessage && `msg=${errorMessage.slice(0, 200)}`,
    traceId && `trace=${traceId}`,
  ]
    .filter(Boolean)
    .join(' ');
  const pulseUrl = process.env.CHORUS_PULSE_URL || 'http://localhost:3475/api/nudge';
  try {
    const ctrl = new AbortController();
    const timeoutId = setTimeout(() => ctrl.abort(), 2000);
    const resp = await fetch(pulseUrl, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'X-Chorus-MCP-Caller': '1' },
      body: JSON.stringify({ from: 'chorus-mcp', to: 'silas', content: summary, traceId }),
      signal: ctrl.signal,
    });
    clearTimeout(timeoutId);
    if (!resp.ok) {
      logEvent('error', 'mcp.notification.failed', { reason: `pulse-${resp.status}`, event, trace_id: traceId });
    }
  } catch (err) {
    logEvent('error', 'mcp.notification.failed', { reason: String(err).slice(0, 200), event, trace_id: traceId });
  }
}

// #2605 — service-lifecycle executor. Wraps agent-state.sh; one function serves
// all 6 per-verb tools.
async function executeServiceLifecycle(
  verb: ServiceVerb,
  service: string,
  role: string,
  repoRoot: string,
): Promise<{ content: Array<{ type: 'text'; text: string }> }> {
  const pathMod = require('path') as typeof import('path');
  const scriptPath = pathMod.join(repoRoot, 'platform', 'scripts', 'agent-state.sh');
  const execFileP = promisify(execFile);
  let stdout: string; // assigned in both try and catch before first read
  let stderr: string;
  let exitCode = 0;
  try {
    const result = await execFileP('bash', [scriptPath, verb, service], {
      env: { ...process.env, DEPLOY_ROLE: role, CHORUS_ROLE: role },
      timeout: 60000,
      maxBuffer: 1024 * 1024,
    });
    stdout = result.stdout || '';
    stderr = result.stderr || '';
  } catch (err) {
    const e = err as NodeJS.ErrnoException & { code?: number; stdout?: string; stderr?: string };
    stdout = e.stdout || '';
    stderr = e.stderr || '';
    exitCode = typeof e.code === 'number' ? e.code : 1;
  }
  if (exitCode === 0) {
    return {
      content: [{ type: 'text' as const, text: JSON.stringify({ ok: true, verb, service, role, stdout: stdout.trim(), stderr: stderr.trim() }) }],
    };
  }
  const reasonMatch = (stderr + stdout).match(/reason=([a-z0-9-]+)/);
  const reason = reasonMatch ? reasonMatch[1] : (exitCode === 2 ? 'usage-error' : 'work-fail');
  throw new Error(`${verb}-fail — reason=${reason} exit=${exitCode}${stderr.trim() ? ' stderr=' + stderr.trim().slice(0, 200) : ''}`);
}

// #3110: werk-binary MCP wrapper executors. Each spawns the verb binary at
// ~/.chorus/bin/<verb> with role + card env wiring, returning structured
// {ok, stdout, stderr, exit} on success or throwing a typed-refusal-shaped
// error on non-zero exit. Parses reason= markers from stderr/stdout so the
// refusal taxonomy on the tool def remains meaningful at the caller side.
async function executeWerkVerb(
  verb: 'werk-build' | 'werk-deploy' | 'werk-pull' | 'werk-commit' | 'werk-push' | 'werk-merge' | 'werk-accept' | 'werk-unpull' | 'werk-review' | 'loom-gemba',
  args: string[],
  role: string,
  cardId: number | undefined,
  extraEnv: Record<string, string>,
): Promise<{ content: Array<{ type: 'text'; text: string }> }> {
  const pathMod = require('path') as typeof import('path');
  const binDir = process.env.CHORUS_BIN || pathMod.join(process.env.HOME || '', '.chorus/bin');
  const binPath = pathMod.join(binDir, verb);
  const execFileP = promisify(execFile);
  let stdout: string; // assigned in both try and catch before first read
  let stderr: string;
  let exitCode = 0;
  try {
    const result = await execFileP(binPath, args, {
      env: {
        ...process.env,
        DEPLOY_ROLE: role,
        CHORUS_ROLE: role,
        CHORUS_HOME: process.env.CHORUS_HOME || '/Users/jeffbridwell/CascadeProjects/chorus',
        CHORUS_WERK_BASE: process.env.CHORUS_WERK_BASE || '/Users/jeffbridwell/CascadeProjects/chorus-werk',
        // #3320 — name the invoker so werk-deploy can detect the self-deploy case
        // (deploying chorus-mcp FROM chorus-mcp) and detach instead of killing this
        // daemon mid-call, which dropped the caller's response (transport-drop class).
        CHORUS_INVOKER: 'chorus-mcp',
        ...extraEnv,
      },
      timeout: 600000,
      maxBuffer: 4 * 1024 * 1024,
    });
    stdout = result.stdout || '';
    stderr = result.stderr || '';
  } catch (err) {
    const e = err as NodeJS.ErrnoException & { code?: number; stdout?: string; stderr?: string };
    stdout = e.stdout || '';
    stderr = e.stderr || '';
    exitCode = typeof e.code === 'number' ? e.code : 1;
  }
  if (exitCode === 0) {
    return {
      content: [{
        type: 'text' as const,
        text: JSON.stringify({ ok: true, verb, role, card_id: cardId, stdout: stdout.trim(), stderr: stderr.trim() }),
      }],
    };
  }
  const combined = stderr + stdout;
  const reasonMatch = combined.match(/reason[=:]\s*([a-z0-9_-]+)/i);
  const reason = reasonMatch ? reasonMatch[1] : (exitCode === 2 ? 'usage-error' : 'work-fail');
  throw new Error(
    `${verb}-fail — reason=${reason} exit=${exitCode}${stderr.trim() ? ' stderr=' + stderr.trim().slice(0, 400) : ''}`,
  );
}

async function executeChorusBuild(
  args: z.infer<typeof BuildInput>,
): Promise<{ content: Array<{ type: 'text'; text: string }> }> {
  return executeWerkVerb('werk-build', [String(args.card_id), args.role], args.role, args.card_id, {});
}

async function executeChorusDeploy(
  args: z.infer<typeof DeployInput>,
): Promise<{ content: Array<{ type: 'text'; text: string }> }> {
  const verbArgs: string[] = [String(args.card_id)];
  if (args.target === 'werk') {
    verbArgs.push('--target', 'werk');
  }
  return executeWerkVerb('werk-deploy', verbArgs, args.role, args.card_id, {});
}

async function executeChorusEnvUp(
  args: z.infer<typeof DeployInput>,
): Promise<{ content: Array<{ type: 'text'; text: string }> }> {
  // werk-deploy env-up <role> <card>: brings the role's variant up on per-role ports.
  // #3239 — card_id is FORWARDED INTO THE ARGV (was omitted), so werk-deploy stands up the
  // card under test's werk, not the first/stale <role>-* dir.
  return executeWerkVerb('werk-deploy', ['env-up', args.role, String(args.card_id)], args.role, args.card_id, {});
}

// #3241 — run the WHOLE pipeline via act, as one MCP verb. This is the encapsulation:
// the caller passes {role, card_id, accepter}; everything the raw `act` CLI needed
// (canonical werk.yml via -W, host-native runner via -P, --input wiring, and a PATH
// that makes chorus-mcp-call.sh + the verb binaries resolvable) lives HERE, not in any
// caller. The act run stops before accept (werk.yml's design); the verb never accepts.
// #3279 — runner env shared by both halves of the split pipeline.
function werkRunnerEnv(home: string, werkBase: string, role: string, runnerPath: string) {
  return {
    ...process.env,
    CHORUS_HOME: home,
    CHORUS_WERK_BASE: werkBase,
    DEPLOY_ROLE: role,
    CHORUS_ROLE: role,
    PATH: runnerPath,
  };
}

// #3279 — Half A: SYNCHRONOUS again, and safe by construction. The demo now PRESENTS
// the variant and EXITS (it no longer blocks for the human go), so this call runs only
// the SHORT front half (commit → build → test → deploy-werk → env-up → demo PRESENT) and
// returns in minutes. It never holds open across a human wait, so #3277's transport drop
// cannot happen — we removed the long hold, we did not paper over it with a detach (which
// cost Jeff his in-session visibility). Jeff sees the presented variant here, in-session;
// his GO re-invokes chorus_werk with go:true, resuming past the demo stop (#3311).
// eslint-disable-next-line complexity -- cohesive werk-pipeline dispatch (verb routing + arg marshalling + result shaping in one place); splitting fragments the one trace (#3429)
async function executeChorusWerk(
  args: z.infer<typeof WerkRunInput>,
  execFileAsync: ExecFileAsync,
): Promise<{ content: Array<{ type: 'text'; text: string }> }> {
  const pathMod = require('path') as typeof import('path');
  const home = process.env.CHORUS_HOME || '/Users/jeffbridwell/CascadeProjects/chorus';
  const werkBase = process.env.CHORUS_WERK_BASE || '/Users/jeffbridwell/CascadeProjects/chorus-werk';
  const binDir = process.env.CHORUS_BIN || pathMod.join(process.env.HOME || '', '.chorus/bin');
  const scriptsDir = pathMod.join(home, 'platform', 'scripts');
  const accepter = args.accepter || 'jeff';
  const workflow = pathMod.join(home, '.github', 'workflows', 'werk.yml');
  const actBin = process.env.CHORUS_ACT_BIN || 'act';
  const runnerPath = [binDir, scriptsDir, '/opt/homebrew/bin', process.env.PATH || ''].filter(Boolean).join(':');
  const landCmd = `chorus_werk {role:"${args.role}", card_id:${args.card_id}, accepter:"${accepter}", go:true}`;
  const actArgs = [
    'workflow_dispatch', '-W', workflow, '-P', 'macos-latest=-self-hosted',
    '--input', `card_id=${args.card_id}`,
    '--input', `role=${args.role}`,
    '--input', `accepter=${accepter}`,
  ];
  let stdout: string; // assigned in both try and catch before first read
  let stderr: string;
  let exitCode = 0;
  try {
    const result = await execFileAsync(actBin, actArgs, {
      env: werkRunnerEnv(home, werkBase, args.role, runnerPath),
      timeout: 0,
      maxBuffer: 16 * 1024 * 1024,
    });
    stdout = result.stdout || '';
    stderr = result.stderr || '';
  } catch (err) {
    const e = err as NodeJS.ErrnoException & { code?: number; stdout?: string; stderr?: string };
    stdout = e.stdout || '';
    stderr = e.stderr || '';
    exitCode = typeof e.code === 'number' ? e.code : 1;
  }
  if (exitCode !== 0) {
    const combined = stderr + stdout;
    const stepMatch = combined.match(/Failure - Main ([a-z0-9-]+)/i);
    const step = stepMatch ? stepMatch[1] : 'unknown';
    throw new Error(`pipeline-fail — reason=pipeline-fail step=${step} exit=${exitCode} — Half A stopped; the variant was not presented. Nothing merged.`);
  }
  return {
    content: [{
      type: 'text' as const,
      text: JSON.stringify({
        ok: true,
        verb: 'chorus_werk',
        phase: 'presented',
        role: args.role,
        card_id: args.card_id,
        accepter,
        go_command: landCmd,
        note: 'Presented and stopped. Nothing is held. On the human GO, re-invoke chorus_werk with go:true — it resumes past the stop (merge → ff-sync → deploy-prod → accept). no-go/more = do nothing; the werk is preserved.',
        stdout: stdout.trim().slice(-4000),
      }),
    }],
  };
}

// #3279/#3193 — Half B: THE GO. Runs werk.yml's go-gated `land` job synchronously (merge → ff-sync →
// deploy-prod → finalize). Short — no human pause inside — so the call returns in
// minutes and cannot drop. Invoked on Jeff's go after he has seen the presented variant.
// Stop-before-accept (DEC-048): it lands to prod and prints the accept command.
async function executeChorusWerkLand(
  args: z.infer<typeof WerkRunInput>,
  execFileAsync: ExecFileAsync,
): Promise<{ content: Array<{ type: 'text'; text: string }> }> {
  const pathMod = require('path') as typeof import('path');
  const home = process.env.CHORUS_HOME || '/Users/jeffbridwell/CascadeProjects/chorus';
  const werkBase = process.env.CHORUS_WERK_BASE || '/Users/jeffbridwell/CascadeProjects/chorus-werk';
  const binDir = process.env.CHORUS_BIN || pathMod.join(process.env.HOME || '', '.chorus/bin');
  const scriptsDir = pathMod.join(home, 'platform', 'scripts');
  const accepter = args.accepter || 'jeff';
  // #3193 — one-file pipeline: the land half is werk.yml's go-gated `land` job
  // (werk-land.yml deleted).
  const workflow = pathMod.join(home, '.github', 'workflows', 'werk.yml');
  const actBin = process.env.CHORUS_ACT_BIN || 'act';
  const runnerPath = [binDir, scriptsDir, '/opt/homebrew/bin', process.env.PATH || ''].filter(Boolean).join(':');
  const actArgs = [
    'workflow_dispatch', '-W', workflow, '-P', 'macos-latest=-self-hosted',
    '--input', `card_id=${args.card_id}`,
    '--input', `role=${args.role}`,
    '--input', `accepter=${accepter}`,
    '--input', 'go=true', // #3193 — selects the go-gated `land` job in the ONE werk.yml
  ];
  let stdout: string; // assigned in both try and catch before first read
  let stderr: string;
  let exitCode = 0;
  try {
    const result = await execFileAsync(actBin, actArgs, {
      env: werkRunnerEnv(home, werkBase, args.role, runnerPath),
      timeout: 0,
      maxBuffer: 16 * 1024 * 1024,
    });
    stdout = result.stdout || '';
    stderr = result.stderr || '';
  } catch (err) {
    const e = err as NodeJS.ErrnoException & { code?: number; stdout?: string; stderr?: string };
    stdout = e.stdout || '';
    stderr = e.stderr || '';
    exitCode = typeof e.code === 'number' ? e.code : 1;
  }
  if (exitCode !== 0) {
    const combined = stderr + stdout;
    const stepMatch = combined.match(/Failure - Main ([a-z0-9-]+)/i);
    const step = stepMatch ? stepMatch[1] : 'unknown';
    throw new Error(`land-fail — reason=land-fail step=${step} exit=${exitCode} — Half B stopped; check what landed before retrying.`);
  }
  return {
    content: [{
      type: 'text' as const,
      text: JSON.stringify({
        ok: true,
        verb: 'chorus_werk',
        phase: 'landed',
        role: args.role,
        card_id: args.card_id,
        accepter,
        note: 'Landed — merged → ff-synced → deployed to prod → accepted. The GO was the accept (DEC-048): werk-accept ran under the accepter named on this call.',
        stdout: stdout.trim().slice(-4000),
      }),
    }],
  };
}

async function executeNudge(
  args: NudgeArgs,
  from: string,
  fetchImpl: FetchImpl,
  pulseUrl: string,
): Promise<{ content: Array<{ type: 'text'; text: string }> }> {
  const { to, message, expects } = args;
  const traceId = mintTraceIdV7();
  logEvent('info', 'mcp.nudge.invoked', { from, to, trace_id: traceId });

  // #2804 — MCP is the only canonical invocation path. Sender-side spine
  // emits fire BEFORE the pulse POST so the audit trail survives even if
  // pulse is unreachable (preserves AC9 belt-and-suspenders from #2727
  // review). Dual-emit nudge.requested (new shape) + nudge.emitted
  // (reader-migration window) — separate cleanup card retires the latter
  // once Clearing tailer / MCP server log fold / operator greps migrate.
  const payload = `from=${from},to=${to},chars=${message.length},trace=${traceId},origin=mcp,content=${message}`;
  await appendChorusLog('nudge.requested', from, { payload });
  await appendChorusLog('nudge.emitted', from, { payload });

  // POST to pulse — pulse worker owns delivery (chorus-inject keystroke).
  // X-Chorus-MCP-Caller header marks this as the canonical caller for the
  // pulse-side caller-check (subsequent commit on this branch hardens the
  // check; today this header is informational and load-bearing for it).
  try {
    const ctrl = new AbortController();
    const timeoutId = setTimeout(() => ctrl.abort(), 5000);
    const resp = await fetchImpl(pulseUrl, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'X-Chorus-Trace-Id': traceId,
        'X-Chorus-MCP-Caller': '1',
      },
      body: JSON.stringify({ from, to, content: message, traceId, expects: expects ?? 'none' }),
      signal: ctrl.signal,
    });
    clearTimeout(timeoutId);
    if (!resp.ok) {
      const errText = resp.text ? await resp.text().catch(() => '') : '';
      throw new Error(`pulse POST returned ${resp.status}: ${errText.slice(0, 200)}`);
    }
    // #3439 AC3: surface the RESOLVED destination pulse computed (which live
    // session/tty the nudge targets, or name-match fallback) so "sent" is no
    // longer blind. Best-effort parse — fall back to the bare role on any miss.
    let dest = `${to}`;
    try {
      const body = (resp.json ? await resp.json() : null) as { resolved?: string } | null;
      if (body && typeof body.resolved === 'string') dest = body.resolved;
    } catch { /* keep bare role */ }
    logEvent('info', 'mcp.nudge.delivered', { from, to, trace_id: traceId, resolved: dest });
    return { content: [{ type: 'text', text: `nudge sent: ${from} → ${dest} (trace=${traceId})` }] };
  } catch (err) {
    const errMsg = err instanceof Error ? err.message : String(err);
    logEvent('error', 'mcp.nudge.failed', { from, to, trace_id: traceId, error: errMsg });
    throw new Error(`nudge delivery failed: ${errMsg}`, { cause: err });
  }
}

// #2652 (AC8) — cards execute helpers. Each spawns the cards bash CLI with
// DEPLOY_ROLE=from so attribution matches whichever role invoked the MCP tool.
// Same canonical chain as bash CLI; MCP is a thin wrapper.

async function execCardsCli(
  verb: string,
  argv: string[],
  from: string,
  execFileAsync: ExecFileAsync,
  cardsPath: string,
  toolName: string,
): Promise<string> {
  const env = {
    ...process.env,
    DEPLOY_ROLE: from,
    CHORUS_CARDS_ORIGIN: 'mcp',
  } as NodeJS.ProcessEnv;
  logEvent('info', `mcp.cards.${verb}.invoked`, { from, argv });
  try {
    const { stdout, stderr } = await execFileAsync(cardsPath, [verb, ...argv], { env, timeout: 10_000 });
    logEvent('info', `mcp.cards.${verb}.delivered`, { from, stdout: stdout.slice(0, 200) });
    return stdout || stderr || `(no output from ${toolName})`;
  } catch (err) {
    // #3293 (bug 1): the agent-card bouncer refusal is an INTENDED outcome, not a
    // crash. The CLI exits non-zero with the [card-approval] ask in its output.
    // Surface it as typed CONTENT (info-logged, not error) so it reaches the filing
    // model — which surfaces it to Jeff (Silas's #3: signal survives) — and stays
    // OFF the mcp.tool.error channel / pain board (the #3278 refusal-dressed-as-crash).
    const e = err as { message?: string; stdout?: string; stderr?: string };
    const out = `${e.stdout || ''}${e.stderr || ''}`;
    if (/REFUSED: agent cards add requires Jeff approval|\[card-approval\]/.test(out)) {
      logEvent('info', `mcp.cards.${verb}.refused`, { from, reason: 'needs-approval' });
      return out || '(card-approval refusal — the structured ask is in the pickup file)';
    }
    // #3347 — surface the REAL failure, never an opaque "Command failed".
    // The 2026-06-11 starvation: cards add exceeded this exec's 10s timeout,
    // execFile KILLED it (a killed process emits no stderr), and the error
    // reached the caller as "Command failed: <command>" with zero diagnosis —
    // twice, while the box starved. Name the timeout-kill case explicitly and
    // append whatever output tail exists.
    const killed = Boolean((err as { killed?: boolean }).killed) || (err as { signal?: string }).signal === 'SIGTERM';
    const outTail = out.trim().slice(-400);
    const errMsg = killed
      ? `cards ${verb} timed out and was killed (exec timeout) — API slow/blocked? (#3347)${outTail ? ` | output tail: ${outTail}` : ''}`
      : `${err instanceof Error ? err.message : String(err)}${outTail ? ` | output tail: ${outTail}` : ''}`;
    logEvent('error', `mcp.cards.${verb}.failed`, { from, error: errMsg.slice(0, 500) });
    throw new Error(`${toolName} failed: ${errMsg}`, { cause: err });
  }
}

async function executeCardsAdd(
  args: z.infer<typeof CardsAddInput>,
  from: string,
  execFileAsync: ExecFileAsync,
  cardsPath: string,
): Promise<{ content: Array<{ type: 'text'; text: string }> }> {
  const argv = [
    args.title,
    '--owner', args.owner,
    '--priority', args.priority,
    '--domain', args.domain,
    '--type', args.type,
    '--origin', args.origin,
    '--desc', args.desc,
  ];
  if (args.sequence) argv.push('--sequence', args.sequence);
  if (args.chunk) argv.push('--chunk', args.chunk);
  if (args.subproduct) argv.push('--subproduct', args.subproduct);
  if (args.subdomain) argv.push('--subdomain', args.subdomain);
  const out = await execCardsCli('add', argv, from, execFileAsync, cardsPath, 'chorus_cards_add');
  return { content: [{ type: 'text', text: out }] };
}

// #2996 — Jeff-attributed add. Hardcodes DEPLOY_ROLE=jeff (the `from` argument
// passed to execCardsCli) so the bouncer's isAgent check returns false and the
// card lands without an approval-ask payload. #3293 removed --quick: Jeff-initiated
// cards now carry the Experience+AC floor too — they skip only the agent
// six-section gate (via attribution), not the substance floor.
async function executeCardAddJeff(
  args: z.infer<typeof CardAddJeffInput>,
  execFileAsync: ExecFileAsync,
  cardsPath: string,
): Promise<{ content: Array<{ type: 'text'; text: string }> }> {
  const argv = [
    args.title,
    '--owner', args.owner,
    '--priority', args.priority,
    '--domain', args.domain,
    '--type', args.type,
    '--origin', args.origin,
  ];
  if (args.desc) argv.push('--desc', args.desc);
  if (args.sequence) argv.push('--sequence', args.sequence);
  if (args.chunk) argv.push('--chunk', args.chunk);
  if (args.subproduct) argv.push('--subproduct', args.subproduct);
  if (args.subdomain) argv.push('--subdomain', args.subdomain);
  // #3025: the cards_add vs card_add_jeff split is the enforced auth-gate
  // boundary, not a prose-only convention. The attribution ('jeff') is
  // hardcoded here regardless of caller, so the bouncer's isAgent check returns
  // false and no approval-ask fires. chorus_cards_add passes the caller's role
  // instead, so the bouncer fires. The distinction lives in code (the literal
  // below), which is why both tools are kept rather than merged.
  const out = await execCardsCli('add', argv, 'jeff', execFileAsync, cardsPath, 'chorus_card_add_jeff');
  return { content: [{ type: 'text', text: out }] };
}

async function executeCardsMove(
  args: z.infer<typeof CardsMoveInput>,
  from: string,
  execFileAsync: ExecFileAsync,
  cardsPath: string,
): Promise<{ content: Array<{ type: 'text'; text: string }> }> {
  // #3025: set-status collapses to one enforced path. Done is owned by
  // chorus_cards_done (the only verb that emits card.accepted, DEC-048).
  // Routing Done through move would silently skip that audit emit.
  if (args.status === 'Done') {
    throw new Error('chorus_cards_move refused: use-cards-done — status=Done must go through chorus_cards_done, which emits the card.accepted spine event subscribers depend on (DEC-048). Moving to Done silently skips that audit emit.');
  }
  const out = await execCardsCli('move', [String(args.id), args.status], from, execFileAsync, cardsPath, 'chorus_cards_move');
  return { content: [{ type: 'text', text: out }] };
}

async function executeCardsDone(
  args: z.infer<typeof CardsDoneInput>,
  from: string,
  execFileAsync: ExecFileAsync,
  cardsPath: string,
): Promise<{ content: Array<{ type: 'text'; text: string }> }> {
  const out = await execCardsCli('done', [String(args.id)], from, execFileAsync, cardsPath, 'chorus_cards_done');
  return { content: [{ type: 'text', text: out }] };
}

async function executeCardsTag(
  args: z.infer<typeof CardsTagInput>,
  from: string,
  execFileAsync: ExecFileAsync,
  cardsPath: string,
): Promise<{ content: Array<{ type: 'text'; text: string }> }> {
  const op = args.op ?? 'add';
  // #3025 AC3-5 / ADR-031: chorus_cards_set is the single writer for label-axis
  // VALUES (sequence/domain/chunk are descriptive properties). cards_tag is the
  // removal verb only — letting it ADD too makes two writers for one field, the
  // exact overlap this card closes. Setting a value routes through cards_set.
  if (op === 'add') {
    throw new Error(
      `chorus_cards_tag refused: use-cards-set — setting a ${args.category} value is owned by chorus_cards_set (one writer per field, ADR-031). Call chorus_cards_set with fields:{${args.category}: "${args.value}"}. chorus_cards_tag handles removal (op=remove).`,
    );
  }
  // op === 'remove' — cards_tag owns removal across every axis via untag.
  const out = await execCardsCli('untag', [String(args.id), `${args.category}:${args.value}`], from, execFileAsync, cardsPath, 'chorus_cards_tag');
  return { content: [{ type: 'text', text: out }] };
}

async function executeCardsSet(
  args: z.infer<typeof CardsSetInput>,
  from: string,
  execFileAsync: ExecFileAsync,
  cardsPath: string,
): Promise<{ content: Array<{ type: 'text'; text: string }> }> {
  // #3025 / ADR-031: cards_set is the single writer for descriptive PROPERTIES —
  // owner, priority, type, origin, title, subdomain, subproduct, and the label
  // axes sequence/domain/chunk (gate-arch ruling: labels are properties, fold
  // into the one setter with typed validation). STATUS is excluded: it's a
  // state machine, not a field — transitions carry the move/accept events a
  // generic setter would silently bypass (today's card.accepted-skipped bug).
  // Lanes go through chorus_cards_move; Done through the accept transaction.
  const keys = Object.keys(args.fields);
  if (keys.some((k) => k.toLowerCase() === 'status')) {
    throw new Error('chorus_cards_set refused: no-status-changes — status is a transition, not a field. Use chorus_cards_move for non-Done lanes, or chorus_cards_done for Done (emits card.accepted).');
  }
  const argv: string[] = [String(args.id)];
  for (const [key, value] of Object.entries(args.fields)) {
    argv.push(`${key}=${value}`);
  }
  const out = await execCardsCli('set', argv, from, execFileAsync, cardsPath, 'chorus_cards_set');
  return { content: [{ type: 'text', text: out }] };
}

async function executeCardsView(
  args: z.infer<typeof CardsViewInput>,
  from: string,
  execFileAsync: ExecFileAsync,
  cardsPath: string,
): Promise<{ content: Array<{ type: 'text'; text: string }> }> {
  const out = await execCardsCli('view', [String(args.id), '--json'], from, execFileAsync, cardsPath, 'chorus_cards_view');
  return { content: [{ type: 'text', text: out }] };
}

/**
 * Build the MCP server with one tool registered. Caller mounts a transport.
 * Caller passes a context-resolver that returns the sender role for a request
 * (read from header / env / session map). Keeps server module pure.
 */
export function buildMcpServer(getCallerRole: () => string, deps: McpServerDeps = {}): Server {
  const execFileAsync: ExecFileAsync = deps.execFileAsync ?? (promisify(execFile) as unknown as ExecFileAsync);
  // #2804 — shimPath retained in McpServerDeps for backward-compat with
  // tests that pass it; unused now that executeNudge POSTs to pulse instead
  // of spawning chorus-hook-shim. Will be removed when bash + shim subcommand
  // delete in this card's later commits.
  void (deps.shimPath ?? resolveShimPath());
  const cardsPath = deps.cardsPath ?? resolveCardsPath();
  const fetchImpl: FetchImpl = deps.fetchImpl ?? (globalThis.fetch as unknown as FetchImpl);
  const apiBase = deps.apiBase ?? 'http://localhost:3340';
  const boardReader: BoardReader = deps.boardReader ?? defaultBoardReader(fetchImpl, apiBase);
  const emitSpineEvent: SpineEmitter = deps.emitSpineEvent ?? defaultSpineEmitter();
  // #3182 — repo root resolved DIRECTLY (env → __dirname), no longer via the
  // git-queue.sh path string. This decouples the MCP layer from git-queue.sh
  // entirely (its last MCP reference; the inline executeAcp caller was retired by
  // #3176). git-queue.sh itself stays for now (session-close.sh, #1623/Phase 2).
   
  const pathRepo = require('path') as typeof import('path');
  const canonicalRepoRoot =
    process.env.CHORUS_ROOT ?? process.env.CHORUS_HOME ?? pathRepo.resolve(__dirname, '..', '..', '..');
  // #2913 — ephemeral-worktree cwd resolver. Default globs chorus-werk/<role>-*;
  // a single match is the role's active card werk, zero/ambiguous falls back to
  // canonical (#2662 cwd=repo-root contract preserved). No CHORUS_WERK_ENABLE
  // flag — the ephemeral model is the model, not an opt-in.
  const resolveWorkingTree: (role: 'kade' | 'wren' | 'silas') => string =
    deps.resolveWorkingTree ?? defaultResolveWorkingTree(canonicalRepoRoot);
   
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
      SERVICE_STATUS_TOOL_DEF,
      SERVICE_START_TOOL_DEF,
      SERVICE_STOP_TOOL_DEF,
      SERVICE_RESTART_TOOL_DEF,
      SERVICE_DEPLOY_TOOL_DEF,
      SERVICE_ROLLBACK_TOOL_DEF,
      CHORUS_BUILD_TOOL_DEF,
      CHORUS_DEPLOY_TOOL_DEF,
      CHORUS_WERK_TOOL_DEF,
      FLOW_REPORT_TOOL_DEF,
      PRINCIPLES_LIST_TOOL_DEF,
      PRINCIPLES_GET_TOOL_DEF,
      PRINCIPLES_CREATE_TOOL_DEF,
      DECISIONS_LIST_TOOL_DEF,
      DECISIONS_GET_TOOL_DEF,
      // #3177: v1 SPARQL ownership tools removed from the advertised list so the
      // team has ONE place to look for ownership — the v2 tree. Use
      // chorus_tree_get / chorus_ownership_lookup (repointed to data/athena/tree.json
      // in #3025) instead. v1 SPARQL stays live for the ~80 detail/facet/coverage
      // consumers v2 doesn't cover yet; only the duplicate ownership path is cut.
      // SUBDOMAINS_LIST_TOOL_DEF,
      // SUBDOMAINS_GET_TOOL_DEF,
      CARDS_ADD_TOOL_DEF,
      CARD_ADD_JEFF_TOOL_DEF,
      CARDS_MOVE_TOOL_DEF,
      CARDS_DONE_TOOL_DEF,
      CARDS_TAG_TOOL_DEF,
      CARDS_SET_TOOL_DEF,
      CARDS_VIEW_TOOL_DEF,
      PRIORITIES_READOUT_TOOL_DEF,
      COMMIT_STATUS_TOOL_DEF,
      COMMIT_TOOL_DEF,
      PULL_CARD_TOOL_DEF,
      LOOM_GEMBA_TOOL_DEF,
      WERK_PUSH_TOOL_DEF,
      WERK_MERGE_TOOL_DEF,
      WERK_ACCEPT_TOOL_DEF,
      UNPULL_CARD_TOOL_DEF,
      WERK_UNPULL_TOOL_DEF,
      WERK_REVIEW_TOOL_DEF,
          DESIGN_REFRESH_TOOL_DEF,
      DOC_CATALOG_ADD_TOOL_DEF,
      LOGS_QUERY_TOOL_DEF,
      LOGS_RECENT_ERRORS_TOOL_DEF,
      LOGS_FOR_CARD_TOOL_DEF,
      LOGS_FOR_TRACE_TOOL_DEF,
      LOGS_FOR_BRANCH_TOOL_DEF,
      PAIN_ROLLUP_TOOL_DEF,
      PAIN_CARD_TOOL_DEF,
      TREE_GET_TOOL_DEF,
      OWNERSHIP_LOOKUP_TOOL_DEF,
      BLAST_RADIUS_TOOL_DEF,
    ],
  }));

  // cog-override: switch dispatcher across 14 MCP tools. Cog-complexity grows
  // 1-per-case; refactoring would mean a name-keyed lookup object losing the
  // per-case zod parsing branches. Acceptable concentration of complexity.
  server.setRequestHandler(CallToolRequestSchema, async (req) => {
    const from = getCallerRole();
    // #3000 — wrap the per-tool dispatch in try/catch + isError check so
    // every error path emits a typed mcp.tool.error spine event. Closes
    // the "MCP errors vaporize behind the boundary" gap named 2026-05-18.
    const errorToolName = req.params.name;
    const errorTraceId = mintTraceIdV7();
    try {
      // cog-override: MCP tool-dispatch switch — one branch per tool by construction; pre-existing, not in #3173 scope
      const result = await (async () => { switch (req.params.name) {
      case 'chorus_nudge_message': {
        const parsed = NudgeInput.safeParse(req.params.arguments);
        if (!parsed.success) {
          throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
        }
        // #2804 — executeNudge POSTs to pulse instead of spawning shim.
        const pulseUrl = process.env.CHORUS_PULSE_URL || 'http://localhost:3475/api/nudge';
        return executeNudge(parsed.data, from, fetchImpl, pulseUrl);
      }
      case 'werk-build': { // #3310 — renamed from chorus_build (ADR-031/032)
        const parsed = BuildInput.safeParse(req.params.arguments);
        if (!parsed.success) {
          throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
        }
        return executeChorusBuild(parsed.data);
      }
      case 'werk-deploy': {
        const parsed = DeployInput.safeParse(req.params.arguments);
        if (!parsed.success) {
          throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
        }
        // #3311 — env-up folded in: one MCP name per binary, subcommand via flag.
        if (parsed.data.env_up) {
          return executeChorusEnvUp(parsed.data);
        }
        return executeChorusDeploy(parsed.data);
      }
      case 'chorus_flow_report': {
        const hours = Number((req.params.arguments as { hours?: number } | undefined)?.hours);
        const h = Number.isFinite(hours) && hours > 0 && hours <= 720 ? String(hours) : '120';
        const pathMod = require('path') as typeof import('path');
        // CHORUS_ROOT first: the variant daemon's plist points it at the card's werk, so a
        // demo-test exercises the WERK's dist (Wren's #3331 seam); canonical daemon's
        // CHORUS_ROOT is canonical. CHORUS_HOME fallback for older contexts.
        const cli = pathMod.join(process.env.CHORUS_ROOT || process.env.CHORUS_HOME || '/Users/jeffbridwell/CascadeProjects/chorus', 'platform/api/dist/flow-report-cli.js');
        const htmlOut = pathMod.join(process.env.HOME || '', '.chorus/reports/card-cycle-report.html');
        const execFileP = promisify(execFile);
        try {
          // process.execPath = the node running THIS daemon — bare 'node' ENOENTs under launchd PATH
          const { stdout } = await execFileP(process.execPath, [cli, '--hours', h, '--html', htmlOut], {
            timeout: 120000, maxBuffer: 16 * 1024 * 1024,
          });
          return { content: [{ type: 'text' as const, text: stdout }] };
        } catch (err) {
          const e = err as { message?: string; stderr?: string };
          throw new Error(`flow-report-fail — ${e.stderr || e.message || 'unknown'}`, { cause: err });
        }
      }
      case 'chorus_werk': {
        const parsed = WerkRunInput.safeParse(req.params.arguments);
        if (!parsed.success) {
          throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
        }
        // #3311 — ONE trigger: go resumes past the demo stop (Half B), else present (Half A).
        if (parsed.data.go) {
          return executeChorusWerkLand(parsed.data, execFileAsync);
        }
        return executeChorusWerk(parsed.data, execFileAsync);
      }
      case 'chorus_service_status':
      case 'chorus_service_start':
      case 'chorus_service_stop':
      case 'chorus_service_restart':
      case 'chorus_service_deploy':
      case 'chorus_service_rollback': {
        const parsed = ServiceLifecycleInput.safeParse(req.params.arguments);
        if (!parsed.success) {
          throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
        }
        const verb = req.params.name.replace('chorus_service_', '') as ServiceVerb;
        if (!SERVICE_LIFECYCLE_VERBS.includes(verb)) {
          throw new Error(`Unknown service verb: ${verb}`);
        }
        const canonicalRoot = process.env.CHORUS_ROOT || '/Users/jeffbridwell/CascadeProjects/chorus';
        return executeServiceLifecycle(verb, parsed.data.service, from, canonicalRoot);
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
      // #3177: v1 SPARQL ownership tools de-listed (see TOOL_DEF registration above).
      // Dispatch cases commented out so a hand-crafted call can't reach the v1 path —
      // it falls through to the unknown-tool error. Use chorus_tree_get /
      // chorus_ownership_lookup (v2) for ownership. executeSubdomainsList/Get +
      // SubdomainsGetInput remain defined but unreferenced (noUnusedLocals off).
      // case 'chorus_subdomains_list':
      //   return executeSubdomainsList(fetchImpl, apiBase, from);
      // case 'chorus_subdomains_get': {
      //   const parsed = SubdomainsGetInput.safeParse(req.params.arguments);
      //   if (!parsed.success) {
      //     throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
      //   }
      //   return executeSubdomainsGet(parsed.data, fetchImpl, apiBase, from);
      // }
      case 'chorus_cards_add': {
        const parsed = CardsAddInput.safeParse(req.params.arguments);
        if (!parsed.success) {
          throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
        }
        return executeCardsAdd(parsed.data, from, execFileAsync, cardsPath);
      }
      case 'chorus_card_add_jeff': {
        const parsed = CardAddJeffInput.safeParse(req.params.arguments);
        if (!parsed.success) {
          throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
        }
        return executeCardAddJeff(parsed.data, execFileAsync, cardsPath);
      }
      case 'chorus_cards_move': {
        const parsed = CardsMoveInput.safeParse(req.params.arguments);
        if (!parsed.success) {
          throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
        }
        return executeCardsMove(parsed.data, from, execFileAsync, cardsPath);
      }
      case 'chorus_cards_done': {
        const parsed = CardsDoneInput.safeParse(req.params.arguments);
        if (!parsed.success) {
          throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
        }
        return executeCardsDone(parsed.data, from, execFileAsync, cardsPath);
      }
      case 'chorus_cards_tag': {
        const parsed = CardsTagInput.safeParse(req.params.arguments);
        if (!parsed.success) {
          throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
        }
        return executeCardsTag(parsed.data, from, execFileAsync, cardsPath);
      }
      case 'chorus_cards_set': {
        const parsed = CardsSetInput.safeParse(req.params.arguments);
        if (!parsed.success) {
          throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
        }
        return executeCardsSet(parsed.data, from, execFileAsync, cardsPath);
      }
      case 'chorus_cards_view': {
        const parsed = CardsViewInput.safeParse(req.params.arguments);
        if (!parsed.success) {
          throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
        }
        return executeCardsView(parsed.data, from, execFileAsync, cardsPath);
      }
      case 'chorus_priorities_readout': {
        const a = (req.params.arguments ?? {}) as { role?: string };
        const role = a.role && ['kade', 'wren', 'silas'].includes(a.role) ? a.role : undefined;
        return executePrioritiesReadout(execFileAsync, role);
      }
      case 'chorus_commit_status': {
        const parsed = CommitStatusInput.safeParse(req.params.arguments);
        if (!parsed.success) {
          throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
        }
        return executeCommitStatus(parsed.data, boardReader, emitSpineEvent);
      }
      case 'werk-commit': {
        const parsed = CommitInput.safeParse(req.params.arguments);
        if (!parsed.success) {
          throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
        }
        // #3178: thin skin → rust werk-commit (v1 executeCommit/git-queue.sh cut).
        const commitArgs = [String(parsed.data.card_id), parsed.data.role];
        if (parsed.data.summary) commitArgs.push(parsed.data.summary);
        return executeWerkVerb('werk-commit', commitArgs, parsed.data.role, parsed.data.card_id, {});
      }
      case 'werk-pull': {
        const parsed = PullCardInput.safeParse(req.params.arguments);
        if (!parsed.success) {
          throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
        }
        // #3135: pull logic lives in the rust `werk-pull` core; the skin just execs it.
        return executeWerkVerb('werk-pull', [String(parsed.data.card_id), parsed.data.role], parsed.data.role, parsed.data.card_id, {});
      }
      case 'loom-gemba': {
        const parsed = LoomGembaInput.safeParse(req.params.arguments);
        if (!parsed.success) {
          throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
        }
        // #3319: thin skin → rust loom-gemba. Observer role rides DEPLOY_ROLE/
        // CHORUS_ROLE env (executeWerkVerb wiring); target is the only argv.
        return executeWerkVerb('loom-gemba', [parsed.data.target], parsed.data.role, undefined, {});
      }
      case 'werk-push': {
        const parsed = WerkPushInput.safeParse(req.params.arguments);
        if (!parsed.success) {
          throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
        }
        // #3178: thin skin → rust werk-push.
        return executeWerkVerb('werk-push', [String(parsed.data.card_id), parsed.data.role], parsed.data.role, parsed.data.card_id, {});
      }
      case 'werk-merge': {
        const parsed = WerkMergeInput.safeParse(req.params.arguments);
        if (!parsed.success) {
          throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
        }
        // #3175: thin skin → rust werk-merge (resolve open PR by HEAD oid, squash, content-verify).
        return executeWerkVerb('werk-merge', [String(parsed.data.card_id), parsed.data.role], parsed.data.role, parsed.data.card_id, {});
      }
      case 'werk-accept': {
        const parsed = WerkAcceptInput.safeParse(req.params.arguments);
        if (!parsed.success) {
          throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
        }
        // #3178: role = builder (werk location); accepter = caller identity via DEPLOY_ROLE (DEC-048).
        return executeWerkVerb('werk-accept', [String(parsed.data.card_id), parsed.data.role], parsed.data.role, parsed.data.card_id, { DEPLOY_ROLE: getCallerRole() });
      }
      case 'werk-review': {
        const a = (req.params.arguments ?? {}) as { mode?: string; role?: string; card_id?: number; verdict?: string; findings?: string };
        if (!a.mode || !a.card_id) {
          throw new Error('Invalid arguments: mode and card_id are required');
        }
        // #3193 — thin skin → rust werk-review (floor | verdict | check).
        if (a.mode === 'floor') {
          if (!a.role) throw new Error('Invalid arguments: floor mode requires role');
          return executeWerkVerb('werk-review', [String(a.card_id), a.role], a.role, a.card_id, {});
        }
        if (a.mode === 'verdict') {
          if (a.verdict !== 'pass' && a.verdict !== 'fail') throw new Error('Invalid arguments: verdict mode requires verdict pass|fail');
          const argv = ['verdict', String(a.card_id), a.verdict, ...(a.findings ? [a.findings] : [])];
          return executeWerkVerb('werk-review', argv, getCallerRole(), a.card_id, {});
        }
        return executeWerkVerb('werk-review', ['check', String(a.card_id)], getCallerRole(), a.card_id, {});
      }
      case 'werk-unpull':
      case 'chorus_unpull_card': {
        // #3299 — thin skin → rust werk-unpull. chorus_unpull_card is the ADR-031
        // deprecation alias: same executor, same contract; drop after callers migrate.
        const parsed = UnpullCardInput.safeParse(req.params.arguments);
        if (!parsed.success) {
          throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
        }
        return executeWerkVerb('werk-unpull', [String(parsed.data.card_id), parsed.data.role], parsed.data.role, parsed.data.card_id, {});
      }
      case 'chorus_design_refresh': {
        const parsed = DesignRefreshInput.safeParse(req.params.arguments);
        if (!parsed.success) {
          throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
        }
        {
          // executeDesignRefresh emits design.refresh.failed itself on typed
          // refusal; errors propagate so the MCP surface returns an error response.
          const fs = require('fs') as typeof import('fs');
          const path = require('path') as typeof import('path');
          const repoRoot = resolveWorkingTree(parsed.data.role);
          const result = await executeDesignRefresh(parsed.data, {
            readFile: (p: string) => fs.readFileSync(p, 'utf8'),
            writeFile: (p: string, content: string) => fs.writeFileSync(p, content, 'utf8'),
            cardsPath,
            designsDir: path.join(repoRoot, 'designing', 'docs'),
            emit: (event: string, fields: Record<string, unknown>) =>
              emitSpineEvent(event, { ...fields, role: parsed.data.role }),
          });
          return {
            content: [{ type: 'text' as const, text: JSON.stringify(result) }],
          };
        }
      }
      case 'chorus_doc_catalog_add': {
        const parsed = DocCatalogAddInput.safeParse(req.params.arguments);
        if (!parsed.success) {
          throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
        }
        return executeDocCatalogAdd(parsed.data, fetchImpl, apiBase, from);
      }
      // #2840 — typed agent surface for log + error investigation. Each
      // handler emits a chorus_logs.queried event so investigation paths
      // are auditable, then returns structured rows or a typed refusal.
      case 'chorus_logs_query':
      case 'chorus_logs_recent_errors':
      case 'chorus_logs_for_card':
      case 'chorus_logs_for_trace':
      case 'chorus_logs_for_branch': {
        const lokiDeps: LogsQueryDeps = {
          fetchImpl: fetchImpl as unknown as typeof fetch,
          lokiUrl: process.env.CHORUS_LOKI_URL || 'http://localhost:3102',
          now: () => Date.now(),
        };
        const tool = req.params.name;
        let result;
        if (tool === 'chorus_logs_query') {
          const parsed = LogsQueryInput.safeParse(req.params.arguments);
          if (!parsed.success) throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
          result = await queryLogs(parsed.data, lokiDeps);
        } else if (tool === 'chorus_logs_recent_errors') {
          const parsed = LogsRecentErrorsInput.safeParse(req.params.arguments);
          if (!parsed.success) throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
          result = await recentErrors(parsed.data, lokiDeps);
        } else if (tool === 'chorus_logs_for_card') {
          const parsed = LogsForCardInput.safeParse(req.params.arguments);
          if (!parsed.success) throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
          result = await logsForCard(parsed.data, lokiDeps);
        } else if (tool === 'chorus_logs_for_branch') {
          const parsed = LogsForBranchInput.safeParse(req.params.arguments);
          if (!parsed.success) throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
          result = await logsForBranch(parsed.data, lokiDeps);
        } else {
          const parsed = LogsForTraceInput.safeParse(req.params.arguments);
          if (!parsed.success) throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
          result = await logsForTrace(parsed.data, lokiDeps);
        }
        emitSpineEvent('chorus_logs.queried', {
          tool,
          from,
          ok: result.ok,
          ...(result.ok ? { count: result.count, truncated: result.truncated } : { reason: result.reason }),
        });
        return { content: [{ type: 'text' as const, text: JSON.stringify(result, null, 2) }] };
      }
      // #3029 — pain board MCP surface. Gateway-to-chorus-api: proxy the existing
      // /api/chorus/pain/* endpoints so any role sees its pain in-session. The
      // rollup logic stays single-sourced in chorus-api (no second copy).
      case 'chorus_pain_rollup':
      case 'chorus_pain_card': {
        const tool = req.params.name;
        let url: string;
        if (tool === 'chorus_pain_rollup') {
          const parsed = PainRollupInput.safeParse(req.params.arguments ?? {});
          if (!parsed.success) throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
          url = `${apiBase}/api/chorus/pain/rollup?window=${encodeURIComponent(parsed.data.window ?? '7d')}`;
        } else {
          const parsed = PainCardInput.safeParse(req.params.arguments);
          if (!parsed.success) throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
          url = `${apiBase}/api/chorus/pain/card/${parsed.data.card_id}`;
        }
        const resp = await fetchImpl(url);
        const body = (await resp.json()) as { ok?: boolean };
        emitSpineEvent('chorus_pain.queried', { tool, from, ok: body?.ok !== false });
        return { content: [{ type: 'text' as const, text: JSON.stringify(body, null, 2) }] };
      }
      case 'chorus_tree_get': {
        const parsed = TreeGetInput.safeParse(req.params.arguments ?? {});
        if (!parsed.success) {
          throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
        }
        try {
          const tree = athenaGetTree();
          emitSpineEvent(EVT_ATHENA_TREE_QUERIED, { tool: 'chorus_tree_get', from, ok: true });
          return { content: [{ type: 'text' as const, text: JSON.stringify(tree) }] };
        } catch (err) {
          const reason = (err as Error).message.includes('ENOENT') ? 'tree-not-found' : 'schema-violation';
          emitSpineEvent(EVT_ATHENA_TREE_QUERIED, {
            tool: 'chorus_tree_get',
            from,
            ok: false,
            reason,
            error: (err as Error).message,
          });
          return {
            content: [{ type: 'text' as const, text: JSON.stringify({ ok: false, reason, error: (err as Error).message }) }],
          };
        }
      }
      case 'chorus_ownership_lookup': {
        const parsed = OwnershipLookupInput.safeParse(req.params.arguments);
        if (!parsed.success) {
          throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
        }
        try {
          const tree = athenaLoadTree();
          const result = athenaLookupOwnership(tree, parsed.data.iri);
          if (!result) {
            emitSpineEvent(EVT_ATHENA_TREE_QUERIED, {
              tool: 'chorus_ownership_lookup',
              from,
              ok: false,
              reason: 'not-found',
              iri: parsed.data.iri,
            });
            return {
              content: [{ type: 'text' as const, text: JSON.stringify({ ok: false, reason: 'not-found', iri: parsed.data.iri }) }],
            };
          }
          emitSpineEvent(EVT_ATHENA_TREE_QUERIED, { tool: 'chorus_ownership_lookup', from, ok: true, iri: parsed.data.iri });
          return { content: [{ type: 'text' as const, text: JSON.stringify(result) }] };
        } catch (err) {
          const reason = 'schema-violation';
          emitSpineEvent(EVT_ATHENA_TREE_QUERIED, {
            tool: 'chorus_ownership_lookup',
            from,
            ok: false,
            reason,
            error: (err as Error).message,
          });
          return {
            content: [{ type: 'text' as const, text: JSON.stringify({ ok: false, reason, error: (err as Error).message }) }],
          };
        }
      }
      case 'chorus_blast_radius': {
        const parsed = BlastRadiusInput.safeParse(req.params.arguments);
        if (!parsed.success) {
          throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
        }
        try {
          const tree = athenaLoadTree();
          const result = athenaComputeBlastRadius(tree, parsed.data.iri);
          if (!result) {
            emitSpineEvent(EVT_ATHENA_TREE_QUERIED, {
              tool: 'chorus_blast_radius',
              from,
              ok: false,
              reason: 'not-found',
              iri: parsed.data.iri,
            });
            return {
              content: [{ type: 'text' as const, text: JSON.stringify({ ok: false, reason: 'not-found', iri: parsed.data.iri }) }],
            };
          }
          emitSpineEvent(EVT_ATHENA_TREE_QUERIED, {
            tool: 'chorus_blast_radius',
            from,
            ok: true,
            iri: parsed.data.iri,
            consumer_count: result.consumers.length,
          });
          return { content: [{ type: 'text' as const, text: JSON.stringify(result) }] };
        } catch (err) {
          const reason = 'schema-violation';
          emitSpineEvent(EVT_ATHENA_TREE_QUERIED, {
            tool: 'chorus_blast_radius',
            from,
            ok: false,
            reason,
            error: (err as Error).message,
          });
          return {
            content: [{ type: 'text' as const, text: JSON.stringify({ ok: false, reason, error: (err as Error).message }) }],
          };
        }
      }
      default:
        throw new Error(`Unknown tool: ${req.params.name}`);
    } })();
      // #3000 — isError-true detection. JSON-RPC tools can return an error
      // envelope (isError: true) without throwing. Treat the same as a throw
      // for spine-emit purposes.
      const r = result as { isError?: boolean; content?: Array<{ type: string; text: string }> };
      if (r.isError === true) {
        const msg = r.content?.[0]?.text ?? 'isError response without content';
        await appendChorusLog('mcp.tool.error', from, {
          tool: errorToolName,
          from,
          error_type: 'is-error',
          error_message: msg.slice(0, 500),
          trace_id: errorTraceId,
        });
      }
      return result;
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : String(err);
      const errorType = /reason=[a-z-]+/.test(errorMessage)
        ? 'subprocess-exit-nonzero'
        : 'throw';
      await appendChorusLog('mcp.tool.error', from, {
        tool: errorToolName,
        from,
        error_type: errorType,
        error_message: errorMessage.slice(0, 500),
        trace_id: errorTraceId,
      });
      throw err; // preserve caller-visible error
    }
  });

  return server;
}
