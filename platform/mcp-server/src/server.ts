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
import { queryLogs, recentErrors, logsForCard, logsForTrace, type LogsQueryDeps } from './handlers/logs-query';
import { resolveGitQueuePath } from './git-queue-path';
import { executeDesignRefresh } from './design-refresh';
// #2997 — athena-tree handler stays in chorus-api for now (heavy fuseki deps).
// chorus-mcp calls it via HTTP from chorus-api instead of importing in-process.
// This keeps chorus-mcp's surface minimal — only depends on cards CLI, git-queue,
// chorus-hook-shim, and Loki HTTP. No fuseki client, no oxigraph, no lancedb.
import {
  loadTree as athenaLoadTree,
  lookupOwnership as athenaLookupOwnership,
  computeBlastRadius as athenaComputeBlastRadius,
} from './athena-tree-stub';

const NudgeInput = z.object({
  to: z.enum(['silas', 'wren', 'kade', 'jeff']).describe('Target role'),
  message: z.string().min(1).describe('Message text the recipient sees'),
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
  opts: { env?: NodeJS.ProcessEnv; timeout?: number; cwd?: string },
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
  // #2682 — git-queue.sh path. The chorus_commit handler spawns it as
  // the canonical commit+push surface. Default resolves to the repo's
  // platform/scripts/git-queue.sh; tests inject a fake path + mock execFileAsync.
  gitQueuePath?: string;
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

// #2682 — chorus_commit (write) input schema. Service derives the active
// card from the board (boardReader), validates branch, runs hooks, commits
// + pushes via existing git-queue.sh. No card_id / branch / force / bypass /
// env-overrides on the wire.
const CommitInput = z.object({
  role: z.enum(['kade', 'wren', 'silas']).describe('Calling role — kade/wren/silas. Service derives the active card from the board.'),
  paths: z.array(z.string().min(1)).min(1).describe('Paths to stage and commit, relative to repo root. Same files passed to `git add` and `git commit -- <paths>` to prevent cross-role staging collisions.'),
  message: z.string().min(1).describe('Commit message body. Service does not modify it; agent supplies the full text including role prefix and card reference.'),
  // #2778 — passes --no-add through to git-queue.sh do_commit (#2731 substrate
  // mechanism, previously not exposed at the MCP surface). Required when the
  // index is already arranged exactly as the commit should look — e.g.,
  // `git rm --cached` of newly-ignored files: `git add` would refuse the
  // ignored paths and the staged deletion could not land.
  no_add: z.boolean().optional().describe('Skip the `git add` step; commit the index as-staged. Use when committing staged deletes of paths that are now in .gitignore — without this, git add refuses ignored paths and the commit cannot land. Default false.'),
}).strict();

// #2688 — chorus_pull (read+rebase) input schema. Sister to chorus_commit:
// thin wrapper over git-queue.sh do_pull. branch/remote optional with sensible
// defaults (current branch + origin). No bypasses on the wire.
const PullInput = z.object({
  role: z.enum(['kade', 'wren', 'silas']).describe('Calling role — kade/wren/silas. DEPLOY_ROLE attribution + spine event role field.'),
  branch: z.string().min(1).optional().describe('Optional branch to pull. Defaults to current HEAD branch via git-queue.sh.'),
  remote: z.string().min(1).optional().describe('Optional remote name. Defaults to origin.'),
}).strict();

// #2750 slice 2 — chorus_acp atomic transaction input.
// #2868 — card_id is now an optional intent-assertion. When present, the
// MCP refuses with `card-mismatch` if the branch-derived card_id differs.
// When absent, derivation is identical to today (branch first, board fallback).
const AcpInput = z.object({
  role: z.enum(['kade', 'wren', 'silas']).describe('Calling role — kade/wren/silas. Card derived from branch then board if card_id absent.'),
  card_id: z.number().int().min(1).optional().describe('Optional intent assertion (#2868). When present, MCP refuses if branch-derived card_id differs.'),
}).strict();

// #2751 — chorus_pull_card atomic transaction input. Role + explicit card_id;
// the /pull skill is the caller, and Jeff or the role names which card.
// No bypasses on the wire — werk-dirty / werk-wrong-branch are typed refusals,
// not flags the caller can suppress.
const PullCardInput = z.object({
  role: z.enum(['kade', 'wren', 'silas']).describe('Calling role — kade/wren/silas. DEPLOY_ROLE attribution + spine event role field.'),
  card_id: z.number().int().positive().describe('Card ID to pull. Must be in Next or Later status with AC + Experience populated.'),
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
  description: 'Use this to deploy a chorus crate end-to-end: chorus-deploy (build + install) + launchctl kickstart + cdhash verify in one atomic flow. Emits paired service.deploy.{started,completed,failed}. Wraps `agent-state.sh deploy <crate>`. Refusal taxonomy: service-not-found | build-fail | kickstart-fail | cdhash-divergence | verify-timeout. Write verb with per-unit authority per #2927 (chorus-api → kade, chorus-hooks → silas, cards-sdk → wren). Do NOT use as part of /acp flow (chorus_acp triggers building-pipeline which deploys via launchd) or to install without rebuilding (no such variant exists — every deploy rebuilds from current werk state).',
  inputSchema: { type: 'object', properties: { service: { type: 'string', minLength: 1, description: 'Crate name (chorus-api, chorus-hooks, chorus-inject)' } }, required: ['service'] },
} as const;

const SERVICE_ROLLBACK_TOOL_DEF = {
  name: 'chorus_service_rollback',
  description: 'Use this to roll back a chorus crate to the prior cdhash from manifest — restore the previous binary, kickstart, verify. Emits paired service.rollback.{started,completed,failed}. Wraps `agent-state.sh rollback <crate>` which invokes `chorus-deploy <crate> --rollback`. Refusal taxonomy: service-not-found | no-prior-cdhash | restore-fail | kickstart-fail | verify-fail. Write verb with per-unit authority per #2927. Do NOT use as a substitute for `git revert` (rollback only restores binary; source state stays at HEAD) or to roll back further than one step (manifest holds only the immediately-prior cdhash today).',
  inputSchema: { type: 'object', properties: { service: { type: 'string', minLength: 1, description: 'Crate name to roll back' } }, required: ['service'] },
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

const CARDS_MOVE_TOOL_DEF = {
  name: 'chorus_cards_move',
  description:
    'Move a card to a new status lane on the kanban board. Use this for routine board flow — Next→WIP when pulling, WIP→Blocked when stuck, Later→Next when triaged. Do NOT use for done-with-evidence — chorus_cards_done is the canonical acceptance path because it emits card.accepted spine event subscribers depend on (DEC-048).',
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
    'Add or remove a tag on an existing card. Use this to set the subproduct, retag during audits, or fix mis-tagged cards. Sequence tags route through the dedicated bulk-tag verb; domain/chunk through label add/remove. Do NOT use for owner/priority/type/origin/title/status — those are structured fields, use chorus_cards_set instead.',
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
    'Atomic update of one or more structured card fields. Use this for owner reassignment, priority bumps, title fixes, status moves, or multi-field changes that should land together. Pass {fields: {priority: "P1", owner: "wren"}}. Do NOT use chorus_cards_tag for owner/priority/type/origin — those are structured fields and chorus_cards_set is the canonical path that emits card.item.set per change.',
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

// #2682 — spine event names. Extracted as consts because sonarjs flags
// duplicated string literals (>5 occurrences across emit + throw paths).
const CHORUS_COMMIT_REFUSED = 'chorus_commit.refused';
const CHORUS_COMMIT_INVOKED = 'chorus_commit.invoked';

// #2682 — chorus_commit (write) tool def. Wraps git-queue.sh commit + push
// behind one declarative call. Service derives card via boardReader; refuses
// with typed reasons (no-wip-card / multi-wip / board-unreachable from
// boardReader, branch-mismatch / hook-fail / push-conflict from git-queue
// exit + stderr classification).
const COMMIT_TOOL_DEF = {
  name: 'chorus_commit',
  description:
    'Use this to commit + push changes for the card you\'re currently building. Service derives the active card from the board (status=WIP, owner=role), validates HEAD matches `<role>/<card-id>`, runs the canonical pre-commit hook chain, and pushes via the serialized queue. Returns SHA + branch + card_id on success, or a typed refusal: no-wip-card / multi-wip / board-unreachable / branch-mismatch / hook-fail / push-conflict. Do NOT use raw `git commit` or `bash git-queue.sh` — those bypass the typed refusal taxonomy and the board-derived card binding.',
  inputSchema: {
    type: 'object',
    properties: {
      role: {
        type: 'string',
        enum: ['kade', 'wren', 'silas'],
        description: 'Calling role — pick one: kade (engineer), wren (PM), silas (architect/ops). Determines DEPLOY_ROLE attribution and which board WIP-card the commit is bound to.',
      },
      paths: {
        type: 'array',
        items: { type: 'string', minLength: 1 },
        minItems: 1,
        description: 'Paths to stage and commit, relative to repo root',
      },
      message: { type: 'string', minLength: 1, description: 'Commit message body' },
      no_add: {
        type: 'boolean',
        description: 'Skip the `git add` step; commit the index as-staged (#2778). Required when committing staged deletes of paths now in .gitignore — git add would refuse the ignored paths and the staged deletion could not land. Default false.',
      },
    },
    required: ['role', 'paths', 'message'],
    additionalProperties: false,
  },
} as const;

// #2687 — classifier collapsed: write-path refusal taxonomy is commit-safety
// only. branch-mismatch retired (we pass --force-branch to git-queue, so its
// internal branch-check no longer surfaces). path-not-found retired (git-add
// failures fold into hook-fail with stderr detail intact). All commit-phase
// non-zero exits → hook-fail. Push-phase failures route through push-conflict
// in executeCommit directly.
const CHORUS_COMMIT_COORDINATION_OBSERVED = 'chorus_commit.coordination_observed';

// #2688 — chorus_pull (read+rebase) tool def. Mirrors chorus_commit shape:
// thin wrapper over git-queue.sh do_pull, --force-branch escape, typed
// refusal taxonomy expanded per #2689 lesson (rebase-conflict | flock-timeout
// | dirty-tree | pull-fail) — narrow taxonomies create false-positives.
const PULL_TOOL_DEF = {
  name: 'chorus_pull',
  description:
    'Use this to pull + rebase the role\'s current branch from origin. Service runs `git pull --rebase` via the existing v2.5 substrate (git-queue.sh do_pull) under the lock. Returns fetched status on success, or a typed refusal: rebase-conflict / flock-timeout / dirty-tree / pull-fail. On rebase-conflict, do_pull aborts cleanly to pre-rebase state. Do NOT use raw `git pull` — that bypasses the lock + classification + spine attribution.',
  inputSchema: {
    type: 'object',
    properties: {
      role: {
        type: 'string',
        enum: ['kade', 'wren', 'silas'],
        description: 'Calling role — pick one: kade (engineer), wren (PM), silas (architect/ops). Determines DEPLOY_ROLE attribution.',
      },
      branch: { type: 'string', minLength: 1, description: 'Optional branch to pull. Defaults to current HEAD branch.' },
      remote: { type: 'string', minLength: 1, description: 'Optional remote name. Defaults to origin.' },
    },
    required: ['role'],
    additionalProperties: false,
  },
} as const;

// #2759 — chorus_unpull_card tool def. /pull's atomic inverse.
const UNPULL_CARD_TOOL_DEF = {
  name: 'chorus_unpull_card',
  description:
    'Use this to reverse a pull and tear down the role\'s WIP card cleanly. Service runs validate (must be WIP + owned by role) + werk pre-flight (refuses werk-dirty) + cards move <id> Next + chorus-werk remove (removes the card\'s ephemeral worktree, deletes the branch, prunes stale admin entries, emits card.branch.closed) + role-state idle + card.unpulled spine event in one atomic transaction. Returns { role, card_id, prior_branch, branch_closed }. Refusal taxonomy: card-not-found | wrong-status | wrong-owner | werk-not-initialized | werk-dirty | move-fail | branch-close-fail. Do NOT use raw cards/git/role-state — those bypass the typed refusal taxonomy and leave stale branches. The /unpull skill calls this and nothing else.',
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

const PULL_CARD_TOOL_DEF = {
  name: 'chorus_pull_card',
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

const ACP_TOOL_DEF = {
  name: 'chorus_acp',
  description:
    'Use this to accept the current WIP card end-to-end. Service derives card_id from HEAD branch (`<role>/<card-id>`) with board fallback, then runs verify-after sequenced steps with typed refusal at each step: commit + push, PR open/merge, cards-done, spine event, branch-close, release-trigger. Idempotent on re-run. Returns { role, card_id, sha, pr_url, branch_closed }. Refusal taxonomy: card-mismatch | hook-fail | commit-fail | push-conflict | push-fail | pr-create-fail | pr-merge-fail | cards-done-fail | branch-close-fail (non-throwing — card is accepted; re-run /acp to retry werk-close idempotently). Pass optional `card_id` to assert intent — MCP refuses with `card-mismatch` if branch-derived id differs (#2868). Do NOT use raw git, gh, or cards CLI — those bypass the typed refusal taxonomy.',
  inputSchema: {
    type: 'object',
    properties: {
      role: {
        type: 'string',
        enum: ['kade', 'wren', 'silas'],
        description: 'Calling role — kade / wren / silas. Card derived from HEAD branch then board fallback if card_id absent.',
      },
      card_id: {
        type: 'integer',
        minimum: 1,
        description: 'Optional intent assertion (#2868). When present, MCP refuses with card-mismatch if branch-derived id differs. When absent, derivation is purely from branch / board.',
      },
    },
    required: ['role'],
    additionalProperties: false,
  },
} as const;

const CHORUS_ACP_REFUSED = 'chorus_acp.refused';
const CARD_ACCEPTED = 'card.accepted';
const CHORUS_PULL_CARD_REFUSED = 'chorus_pull_card.refused';
const CARD_PULLED = 'card.pulled';

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

const CHORUS_UNPULL_CARD_REFUSED = 'chorus_unpull_card.refused';
const CARD_UNPULLED = 'card.unpulled';
// #2752 — sonarjs no-duplicate-string: extract literals appearing >5x
const FORCE_BRANCH_FLAG = '--force-branch';
// #2799 — force-with-lease passthrough for the post-rebase push step.
// chorus_commit / chorus_acp do an internal `pull --rebase` before push;
// when the local branch already had commits on origin, the rebase changes
// local SHAs and a regular push hits non-fast-forward. Force-with-lease
// is the safe variant — refuses on concurrent peer push, pushes our
// rebased history otherwise. Closes the variant-recovery class today
// papered over by /tmp/wren-N-push.sh scripts running raw git outside
// the typed surface.
const FORCE_WITH_LEASE_FLAG = '--force-with-lease';
const ALREADY_MERGED = 'already-merged';
const STEP_PUSH = 'push';
// Step-name constants — extracted to satisfy sonarjs/no-duplicate-string (threshold 5).
const STEP_WERK_CLOSE = 'werk-close';
const STEP_WERK_PREFLIGHT = 'werk-preflight';
const STEP_CARDS_MOVE = 'cards-move';
const STEP_ROLE_STATE = 'role-state';
// Script / directory name used in path.join across pull, acp, and unpull flows.
const CHORUS_WERK = 'chorus-werk';
// Spine event emitted by all three athena query tools.
const EVT_ATHENA_TREE_QUERIED = 'athena.tree.queried';

interface AcpArgs {
  role: 'kade' | 'wren' | 'silas';
  card_id?: number; // #2868 — optional intent assertion
}

// #2688 — chorus_pull spine event names. Same extraction reason as commit
// (sonarjs no-duplicate-string).
const CHORUS_PULL_REFUSED = 'chorus_pull.refused';
const CHORUS_PULL_FETCHED = 'chorus_pull.fetched';
const CHORUS_PULL_REBASE_ATTEMPTED = 'chorus_pull.rebase.attempted';
const CHORUS_PULL_REBASE_ABORTED = 'chorus_pull.rebase.aborted';
const CHORUS_PULL_COORDINATION_OBSERVED = 'chorus_pull.coordination_observed';

interface PullArgs {
  role: 'kade' | 'wren' | 'silas';
  branch?: string;
  remote?: string;
}

// #2688 — pull-phase classifier. 4 labels per #2689 lesson (narrow taxonomy
// reproduces classifier-collapse false-positives). Each pattern requires a
// failure marker, not just a substring match.
function classifyPullFailure(stderr: string): 'rebase-conflict' | 'flock-timeout' | 'dirty-tree' | 'pull-fail' {
  if (/CONFLICT|merge conflict|could not apply/i.test(stderr)) return 'rebase-conflict';
  if (/timeout.*lock|holding the lock/i.test(stderr)) return 'flock-timeout';
  if (/unstaged changes|cannot pull with rebase|please commit or stash/i.test(stderr)) return 'dirty-tree';
  return 'pull-fail';
}

interface CommitArgs {
  role: 'kade' | 'wren' | 'silas';
  paths: string[];
  message: string;
  no_add?: boolean;
}

// #2689/#2697 — classifiers extracted from executeCommit to keep cognitive
// complexity under threshold. Each returns the typed reason for its phase.
function classifyCommitFailure(stderr: string): 'hook-fail' | 'commit-fail' {
  // #2699 — tightened from /^pre-commit:|^.. blocked|hook failed/i. Old regex
  // matched any pre-commit-prefixed line (incl. warnings) and the bare 'hook
  // failed' substring anywhere. New form requires a failure marker (red circle,
  // X, 'failed', 'blocked') on the same line as the 'pre-commit:' prefix.
  // Wren observed the over-match during #2689 acp dogfood 2026-05-03.
  return /pre-commit:.*(?:🔴|❌|failed|blocked)/i.test(stderr) ? 'hook-fail' : 'commit-fail';
}

function classifyPushFailure(stderr: string): 'push-conflict' | 'push-fail' {
  return /rebase|conflict|merge/i.test(stderr) ? 'push-conflict' : 'push-fail';
}

function extractStderr(err: unknown): string {
  return (err as { stderr?: string }).stderr ?? (err instanceof Error ? err.message : String(err));
}

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
export function defaultResolveWorkingTree(canonicalRoot: string): (role: 'kade' | 'wren' | 'silas') => string {
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  const fs = require('node:fs') as typeof import('node:fs');
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  const path = require('node:path') as typeof import('node:path');

  return (role: 'kade' | 'wren' | 'silas'): string => {
    // CHORUS_WERK_BASE convention: sibling of canonical, parent dir + /chorus-werk/
    const werkBase = path.join(path.dirname(canonicalRoot), CHORUS_WERK);
    let matches: string[] = [];
    try {
      matches = fs.readdirSync(werkBase, { withFileTypes: true })
        .filter((e) => e.isDirectory() && e.name.startsWith(`${role}-`))
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

// cog-override: commit orchestration handles multiple typed-refusal branches — structurally complex
async function executeCommit(
  args: CommitArgs,
  boardReader: BoardReader,
  emit: SpineEmitter,
  execFileAsync: ExecFileAsync,
  gitQueuePath: string,
  resolveWorkingTree: (role: 'kade' | 'wren' | 'silas') => string,
): Promise<{ content: Array<{ type: 'text'; text: string }> }> {
  const { role, paths, message, no_add } = args;

  // #2857 — flow trace. Mint trace_id at entry, wrap emit so every event
  // carries it. card_id resolves via board lookup below; re-wrap once known.
  const baseEmit = emit;
  const trace_id = mintTraceIdV7();
  emit = createSpineEmitter(trace_id, baseEmit);

  // #2687 — Step 1: best-effort board lookup for spine attribution. NEVER
  // refuses. Coordination state (no-wip-card / multi-wip / board-unreachable)
  // is observed via chorus_commit.coordination_observed event; the commit
  // proceeds regardless. This is the v3-regression strip: the write-path
  // refusal taxonomy is commit-safety only (hook-fail / push-conflict).
  const board = await boardReader(role);
  let cardId: number | null = null;
  if (!board.ok) {
    emit(CHORUS_COMMIT_COORDINATION_OBSERVED, { role, reason: board.reason, detail: board.detail });
  } else if (board.cards.length === 0) {
    emit(CHORUS_COMMIT_COORDINATION_OBSERVED, { role, reason: 'no-wip-card' });
  } else if (board.cards.length > 1) {
    const ids = board.cards.map((c) => c.id).join(',');
    emit(CHORUS_COMMIT_COORDINATION_OBSERVED, { role, reason: 'multi-wip', card_ids: ids });
    cardId = board.cards[0].id; // best-guess attribution for spine
  } else {
    cardId = board.cards[0].id;
  }
  if (cardId !== null) emit = createSpineEmitter(trace_id, baseEmit, cardId);
  const branch = cardId ? `${role}/${cardId}` : `${role}/uncoordinated`;

  // #2662 — chorus-api's launchctl PATH puts /opt/homebrew/bin first, which
  // is Node 23. The chorus-api process itself runs the team's nvm Node 20
  // (absolute path in launch plist), but subprocess PATH-resolution of
  // `node`/`npx`/`npm` (used by pre-commit's `npx jest`) picks up the
  // Homebrew binary, breaking native modules compiled for Node 20.
  // Prepend the parent node's bin dir so the subprocess chain stays on the
  // same Node version as chorus-api itself.
  const path = require('path') as typeof import('path');
  const parentNodeBinDir = path.dirname(process.execPath);
  // #2687 — also prepend the user's cargo bin so chorus-hooks rust checks
  // (clippy-ratchet) find cargo. chorus-api's launchctl PATH doesn't include
  // ~/.cargo/bin; subprocess pre-commit hits FileNotFoundError on cargo.
  // Same launchctl-PATH-leak shape as #2662's Node 20/23 mismatch.
  const cargoBinDir = `${process.env.HOME ?? ''}/.cargo/bin`;
  const env = {
    ...process.env,
    DEPLOY_ROLE: role,
    CHORUS_TRACE_ID: trace_id,
    ...(cardId !== null ? { CHORUS_CARD_ID: String(cardId) } : {}),
    PATH: `${parentNodeBinDir}:${cargoBinDir}:${process.env.PATH ?? ''}`,
  } as NodeJS.ProcessEnv;

  // #2662 — git-queue.sh stages paths via `git add <path>` which resolves
  // them relative to cwd. chorus-api runs from platform/api, so without
  // an explicit cwd, paths like "skills/acp/SKILL.md" became
  // "platform/api/skills/acp/SKILL.md" (404).
  // #2750 — werk-aware: when the role's flag is on, repoRoot is the role's
  // werk; else canonical (#2662 contract). resolveWorkingTree owns the
  // decision; chorus_commit just routes cwd accordingly.
  const repoRoot = resolveWorkingTree(role as 'kade' | 'wren' | 'silas');

  // Step 2 — commit via git-queue.sh. `<paths> -- -m <message>` is the contract.
  // #2687 — pass --force-branch so git-queue's branch-check (coordination
  // refusal) doesn't surface. Branch naming is observed via spine, not
  // enforced at the write surface.
  let commitStdout: string;
  try {
    // #2778 — --no-add (after --force-branch) routes to git-queue.sh do_commit
    // skip-add path (#2731). Required for committing staged deletes of now-
    // ignored paths; without it, git add refuses ignored paths and the staged
    // deletion cannot land via the typed surface.
    const commitArgs = no_add
      ? ['commit', FORCE_BRANCH_FLAG, '--no-add', ...paths, '--', '-m', message]
      : ['commit', FORCE_BRANCH_FLAG, ...paths, '--', '-m', message];
    const { stdout } = await execFileAsync(gitQueuePath, commitArgs, { env, timeout: 30_000, cwd: repoRoot });
    commitStdout = stdout;
  } catch (err) {
    const stderr = extractStderr(err);
    const reason = classifyCommitFailure(stderr);
    emit(CHORUS_COMMIT_REFUSED, { role, card_id: cardId, reason, detail: stderr.slice(0, 500) });
    throw new Error(`chorus_commit refused: ${reason} — ${stderr.split('\n')[0]}`);
  }

  // Extract SHA from `[branch sha] message` line (git's standard commit output).
  const shaMatch = commitStdout.match(/\[\S+\s+([a-f0-9]+)\]/);
  const sha = shaMatch ? shaMatch[1] : 'unknown';

  // #2699 — Capture HEAD ref name immediately after commit lands. Defensive
  // against Mode-A: a peer's checkout between this point and the push step
  // would silently move HEAD; bare `git push` would then push the wrong branch
  // (because #2689's --force-branch escape disabled do_push's check_branch).
  // #2705 — Captured ref passes via explicit --branch arg (substrate-uniform
  // with --force-branch shape; env-on-the-wire retired per silas gate-arch).
  let pushRef = '';
  try {
    const { stdout: headOut } = await execFileAsync('git', ['rev-parse', '--abbrev-ref', 'HEAD'], { env, cwd: repoRoot, timeout: 5_000 });
    pushRef = headOut.trim();
  } catch {
    // capture failed — fall through to bare push (current behavior, accepts Mode-A residual)
  }

  // Step 3 — push via git-queue.sh (rebase-on-conflict, race-safe under lock).
  // #2689 — pass --force-branch (mirrors #2687's commit-side fix). Without it,
  // a Mode-A bump between commit and push triggered do_push's check_branch and
  // surfaced as a false-positive push-conflict (6001a6be / b53a7fe5 / e19588b0).
  // #2705 — --branch <ref> targets origin REF:REF when set; mirrors the
  // explicit-arg shape, no env-on-the-wire.
  // #2799 — pass --force-with-lease so the rebase-then-push path
  // (do_push: pull --rebase && push) survives the rebased-local-vs-origin
  // case without falling back to a /tmp script. Order matches git-queue.sh
  // parser: --force-branch, --force-with-lease, --branch <ref>.
  const pushArgs = pushRef
    ? [STEP_PUSH, FORCE_BRANCH_FLAG, FORCE_WITH_LEASE_FLAG, '--branch', pushRef]
    : [STEP_PUSH, FORCE_BRANCH_FLAG, FORCE_WITH_LEASE_FLAG];
  try {
    await execFileAsync(gitQueuePath, pushArgs, { env, timeout: 60_000, cwd: repoRoot });
  } catch (err) {
    const stderr = extractStderr(err);
    const reason = classifyPushFailure(stderr);
    emit(CHORUS_COMMIT_REFUSED, { role, card_id: cardId, reason, detail: stderr.slice(0, 500) });
    throw new Error(`chorus_commit refused: ${reason} — ${stderr.split('\n')[0]}`);
  }

  // Step 4 — success. Emit invoked and return structured payload.
  emit(CHORUS_COMMIT_INVOKED, { role, card_id: cardId, paths_count: paths.length, sha });
  return {
    content: [
      {
        type: 'text',
        text: JSON.stringify({ role, card_id: cardId, branch, sha }, null, 2),
      },
    ],
  };
}

async function executePull(
  args: PullArgs,
  boardReader: BoardReader,
  emit: SpineEmitter,
  execFileAsync: ExecFileAsync,
  gitQueuePath: string,
  resolveWorkingTree: (role: 'kade' | 'wren' | 'silas') => string,
): Promise<{ content: Array<{ type: 'text'; text: string }> }> {
  const { role, branch, remote } = args;

  // Best-effort board lookup for spine attribution. Same shape as executeCommit
  // (#2687 strip): coordination state observed via chorus_pull.coordination_observed,
  // pull proceeds regardless. Refusal taxonomy is pull-safety only.
  const board = await boardReader(role);
  let cardId: number | null = null;
  if (!board.ok) {
    emit(CHORUS_PULL_COORDINATION_OBSERVED, { role, reason: board.reason, detail: board.detail });
  } else if (board.cards.length === 0) {
    emit(CHORUS_PULL_COORDINATION_OBSERVED, { role, reason: 'no-wip-card' });
  } else if (board.cards.length > 1) {
    const ids = board.cards.map((c) => c.id).join(',');
    emit(CHORUS_PULL_COORDINATION_OBSERVED, { role, reason: 'multi-wip', card_ids: ids });
    cardId = board.cards[0].id;
  } else {
    cardId = board.cards[0].id;
  }

  // Same env shape as executeCommit (#2662 + #2687 PATH leaks).
  const path = require('path') as typeof import('path');
  const parentNodeBinDir = path.dirname(process.execPath);
  const cargoBinDir = `${process.env.HOME ?? ''}/.cargo/bin`;
  const env = {
    ...process.env,
    DEPLOY_ROLE: role,
    PATH: `${parentNodeBinDir}:${cargoBinDir}:${process.env.PATH ?? ''}`,
  } as NodeJS.ProcessEnv;
  // #2750 — werk-aware cwd (mirror of executeCommit's change).
  const repoRoot = resolveWorkingTree(role as 'kade' | 'wren' | 'silas');

  // Emit attempted before the call so audit captures the intent even if the
  // call throws unexpectedly. Pull-rebase is the operation; chorus_pull.fetched
  // fires on success below.
  emit(CHORUS_PULL_REBASE_ATTEMPTED, { role, card_id: cardId, branch, remote });

  const pullArgs: string[] = ['pull', FORCE_BRANCH_FLAG];
  if (branch) pullArgs.push('--branch', branch);
  if (remote) pullArgs.push('--remote', remote);

  try {
    await execFileAsync(gitQueuePath, pullArgs, { env, timeout: 60_000, cwd: repoRoot });
  } catch (err) {
    const stderr = extractStderr(err);
    const reason = classifyPullFailure(stderr);
    if (reason === 'rebase-conflict') {
      // do_pull aborts cleanly to pre-rebase state on conflict; the abort
      // event lets observers tail recovery without parsing stderr.
      emit(CHORUS_PULL_REBASE_ABORTED, { role, card_id: cardId, detail: stderr.slice(0, 500) });
    }
    emit(CHORUS_PULL_REFUSED, { role, card_id: cardId, reason, detail: stderr.slice(0, 500) });
    throw new Error(`chorus_pull refused: ${reason} — ${stderr.split('\n')[0]}`);
  }

  emit(CHORUS_PULL_FETCHED, { role, card_id: cardId, branch, remote });
  return {
    content: [
      { type: 'text', text: JSON.stringify({ role, card_id: cardId, branch, remote, status: 'fetched' }, null, 2) },
    ],
  };
}

// #2750 slice 2 — atomic /acp transaction. Wraps the existing executeCommit
// path then runs PR-merge + cards-done + spine + werk-close as one
// deterministic flow. Idempotent on re-run: gh pr view detects existing PR.
// cog-override: orchestrates 7-step acp transaction with per-step idempotency, error classification, and werk routing; splitting obscures linear flow (#2627)
async function executeAcp(
  args: AcpArgs,
  boardReader: BoardReader,
  emit: SpineEmitter,
  execFileAsync: ExecFileAsync,
  gitQueuePath: string,
  cardsPath: string,
  resolveWorkingTree: (role: 'kade' | 'wren' | 'silas') => string,
): Promise<{ content: Array<{ type: 'text'; text: string }> }> {
  const { role } = args;
  // #2925 — when caller asserts card_id, prefer that card's exact ephemeral
  // werk over the glob resolver's fallback-to-canonical. Closes a bootstrap
  // recursion: a role with 2+ active werks (e.g. silas/2605 + silas/2925)
  // hits the ambiguous-multiple case and the default returns canonical →
  // executeAcp runs git from main → pre-push hook #2598 refuses
  // "branch 'main' does not match role prefix 'silas/'".
  const acpRequireBoot = require as NodeJS.Require;
  const acpFsBoot = acpRequireBoot('node:fs') as typeof import('node:fs');
  const acpPathBoot = acpRequireBoot('node:path') as typeof import('node:path');
  let repoRoot = resolveWorkingTree(role);
  if (args.card_id !== undefined) {
    const werkBase = process.env.CHORUS_WERK_BASE
      ?? acpPathBoot.join(
        acpPathBoot.dirname(process.env.CHORUS_ROOT ?? '/Users/jeffbridwell/CascadeProjects/chorus'),
        CHORUS_WERK
      );
    const cardWerk = acpPathBoot.join(werkBase, `${role}-${args.card_id}`);
    if (acpFsBoot.existsSync(cardWerk)) {
      repoRoot = cardWerk;
    }
  }
  // #2857 — flow trace. Mint trace_id at handler entry, wrap emit so every
  // event in this flow carries it. Re-wrap with card_id once derived. Bash
  // subprocesses (git-queue, gh, cards CLI) inherit via CHORUS_TRACE_ID +
  // CHORUS_CARD_ID env vars set on the env objects below.
  const baseEmit = emit;
  const trace_id = mintTraceIdV7();
  // #2752 bug-4 — step-by-step logging. Each step emits .started before the
  // subprocess and .completed after success. Refusals already named the
  // step. Now any failure mode shows the exact step that ran/failed without
  // re-running with verbose flags.
  let cardId: number | null = null;
  emit = createSpineEmitter(trace_id, baseEmit);
  // #2931: auto-inject duration_ms on every completed step. Map records the
  // wall-clock start of each `${step}.started` emit; on `${step}.completed`
  // we subtract and attach duration_ms before emitting. Per-step keys so
  // overlapping or re-entered steps (idempotent re-runs of commit/push)
  // remeasure cleanly. No shared state across handler invocations — Map is
  // closure-local to this chorus_acp call.
  const stepStartedAt = new Map<string, number>();
  const stepEmit = (step: string, status: 'started' | 'completed', detail?: Record<string, unknown>) => {
    if (status === 'started') stepStartedAt.set(step, Date.now());
    const dur = status === 'completed' ? stepStartedAt.get(step) : undefined;
    const duration = dur !== undefined ? { duration_ms: Date.now() - dur } : {};
    emit(`chorus_acp.${step}.${status}`, { role, card_id: cardId, ...duration, ...(detail ?? {}) });
  };

  emit('chorus_acp.invoked', { role, repo_root: repoRoot });

  // Step 0 — derive cardId from HEAD branch FIRST (#2782). The branch name
  // `<role>/<card-id>` is the source-of-truth for which card a commit is
  // for; the board is a coordination view that can lag (just-merged card
  // moves to Done before the next /acp invocation). Pre-#2782 the cardId
  // was board-only, gated on cards.length===1, which silently dropped to
  // null on idempotent re-runs (board has 0 WIP) AND on multi-WIP races.
  // Result: cards-done step skipped, branch_closed=true returned, card
  // stuck at "Later" — receipt: #2777, #2778, #2779 on 2026-05-07.
  const acpRequire = require as NodeJS.Require;
  const acpPathMod = acpRequire('node:path') as typeof import('node:path');
  const _envForBranchProbe = {
    ...process.env,
    DEPLOY_ROLE: role,
    CHORUS_TRACE_ID: trace_id,
    PATH: `${acpPathMod.dirname(process.execPath)}:${process.env.HOME ?? ''}/.cargo/bin:${process.env.PATH ?? ''}`,
  } as NodeJS.ProcessEnv;
  let initialBranch = '';
  try {
    const { stdout: branchOut } = await execFileAsync('git', ['rev-parse', '--abbrev-ref', 'HEAD'], { env: _envForBranchProbe, cwd: repoRoot, timeout: 5_000 });
    initialBranch = branchOut.trim();
  } catch {
    /* no branch; fall back to board */
  }
  const branchMatch = initialBranch.match(/^([a-z]+)\/(\d+)$/);
  if (branchMatch && branchMatch[1] === role) {
    cardId = parseInt(branchMatch[2], 10);
  }

  // Step 1 — board lookup. Used as a fallback only when branch didn't yield
  // a cardId (e.g. branch is `main` or some non-canonical name). Branch wins.
  stepEmit('board-lookup', 'started');
  const board = await boardReader(role);
  if (cardId === null && board.ok && board.cards.length === 1) cardId = board.cards[0].id;
  // #2857 — re-wrap emit with cardId now that it's resolved, so every
  // downstream emit carries both trace_id and card_id automatically.
  if (cardId !== null) emit = createSpineEmitter(trace_id, baseEmit, cardId);
  stepEmit('board-lookup', 'completed', { card_id: cardId, board_ok: board.ok, source: branchMatch ? 'branch' : 'board' });

  // #2868 — intent-assertion guard. If the caller passed args.card_id, it
  // must match the derived cardId. Silent substitution is the failure
  // mode this card closes (today: wren werk on wren/2851, /acp 2847 ran
  // against 2851 with no signal). Refuse loudly with both ids named.
  if (args.card_id !== undefined && cardId !== null && args.card_id !== cardId) {
    emit('chorus_acp.refused', {
      role,
      step: 'intent-check',
      reason: 'card-mismatch',
      requested_card_id: args.card_id,
      branch_card_id: cardId,
      detail: `Caller asked for card ${args.card_id} but werk branch ${initialBranch} derives card ${cardId}. Repoint werk to ${role}/${args.card_id} or omit card_id to accept current branch.`,
    });
    throw new Error(`chorus_acp refused: card-mismatch — requested=${args.card_id} branch=${cardId}`);
  }

  // #2923: the fast-path (alreadyMerged → skip commit/push/PR → cards-done)
  // was removed. It was a false-success shortcut — `git cherry` showing no
  // commits ahead is identical whether the work is already merged OR never
  // committed, so an uncommitted werk took the shortcut and got marked Done
  // with nothing shipped. The normal path below is idempotent on re-runs
  // (commit catches "nothing to commit", push catches "up to date",
  // pr-merge catches "already merged"), so it handles the already-merged
  // case correctly without a separate branch.

  // Step 2 — commit + push via existing executeCommit machinery. Reuse the
  // CommitArgs path: paths=['.'] commits everything staged in werk; the role
  // already staged what they want via Edit/Write before invoking /acp.
  // The skill's pre-flight ensures clean diff matches intent.
  // We can't reuse executeCommit directly because it takes pre-set paths;
  // for /acp the contract is "everything in the current branch."
  // Run git-queue.sh commit with `.` as the path argument.
  const path = require('path') as typeof import('path');
  const parentNodeBinDir = path.dirname(process.execPath);
  const cargoBinDir = `${process.env.HOME ?? ''}/.cargo/bin`;
  const env = {
    ...process.env,
    DEPLOY_ROLE: role,
    CHORUS_TRACE_ID: trace_id,
    ...(cardId !== null ? { CHORUS_CARD_ID: String(cardId) } : {}),
    PATH: `${parentNodeBinDir}:${cargoBinDir}:${process.env.PATH ?? ''}`,
  } as NodeJS.ProcessEnv;

  let sha = 'unknown';
  let branch = '';

  // Detect existing branch first (if commit already landed in a previous run)
  stepEmit('detect-branch', 'started');
  try {
    const { stdout: refOut } = await execFileAsync('git', ['rev-parse', '--abbrev-ref', 'HEAD'], { env, cwd: repoRoot, timeout: 5_000 });
    branch = refOut.trim();
    stepEmit('detect-branch', 'completed', { branch });
  } catch {
    /* fall through; will fail at commit */
  }

  // #2943 — idempotent re-run path: detect "card already accepted, only
  // werk-close pending." Pre-#2943, a transient branch-close failure left
  // an orphan remote branch that no path could clean up from agent sessions
  // (#2598 hook prohibits direct git push --delete). Re-running /acp on the
  // same card retried commit/push/PR, which all returned idempotent no-ops,
  // and then re-attempted werk-close — works in principle but burned cycles
  // re-validating already-merged state. New shape: detect the condition
  // upfront (card status=Done on the board AND remote branch <role>/<id>
  // still exists), skip directly to werk-close, return cleanly.
  if (cardId !== null && board.ok) {
    const acceptedCard = board.cards.find((c) => c.id === cardId);
    const cardIsDone = acceptedCard === undefined; // boardReader returns WIP-only; absent = not WIP = Done or other
    if (cardIsDone) {
      const expectedBranch = `${role}/${cardId}`;
      let remoteHasOrphan = false;
      try {
        const { stdout: lsOut } = await execFileAsync('git', ['ls-remote', '--heads', 'origin', expectedBranch], { env, cwd: repoRoot, timeout: 10_000 });
        remoteHasOrphan = lsOut.trim().length > 0;
      } catch { /* ls-remote failure is itself a signal we shouldn't run cleanup */ }

      if (remoteHasOrphan) {
        emit('chorus_acp.idempotent-cleanup.detected', { role, card_id: cardId, branch: expectedBranch });
        stepEmit(STEP_WERK_CLOSE, 'started');
        try {
          const chorusWerkPath = path.join(repoRoot, 'platform', 'scripts', CHORUS_WERK);
          await execFileAsync(chorusWerkPath, ['remove', role, String(cardId)], { env, timeout: 30_000 });
          stepEmit(STEP_WERK_CLOSE, 'completed', { branch_closed: true, idempotent_cleanup: true });
          emit('chorus_acp.completed', { role, card_id: cardId, sha: 'idempotent-cleanup', pr_url: 'idempotent-cleanup', branch_closed: true });
          return {
            content: [
              { type: 'text', text: JSON.stringify({ role, card_id: cardId, sha: 'idempotent-cleanup', pr_url: 'idempotent-cleanup', branch_closed: true, idempotent_cleanup: true }, null, 2) },
            ],
          };
        } catch (err) {
          const stderr = extractStderr(err);
          stepEmit(STEP_WERK_CLOSE, 'completed', { branch_closed: false, error: stderr.slice(0, 200), idempotent_cleanup: true });
          emit(CHORUS_ACP_REFUSED, {
            role,
            card_id: cardId,
            step: STEP_WERK_CLOSE,
            reason: 'branch-close-fail',
            detail: stderr.slice(0, 500),
            recoverable: true,
            recovery_hint: `re-run \`/acp ${cardId}\` again, or have silas (DEC-022) run \`git push origin --delete ${expectedBranch}\``,
            idempotent_cleanup_attempted: true,
          });
          throw new Error(`chorus_acp refused: branch-close-fail — ${stderr.split('\n')[0]}`);
        }
      }
    }
  }

  // Commit (skip if no staged changes — gh push will be no-op).
  // #2931 — write Chorus-Trace-Id / Chorus-Card-Id git trailers so the build
  // pipeline picks up the ACP trace_id from the commit it's building. Without
  // this, build/deploy events mint their own trace_id and ACP→build→deploy
  // can't be joined in chorus_logs_for_trace.
  stepEmit('commit', 'started', { branch });
  try {
    const trailers = [`Chorus-Trace-Id: ${trace_id}`];
    if (cardId !== null) trailers.push(`Chorus-Card-Id: ${cardId}`);
    const commitMessage = `${role}: acp #${cardId ?? 'unknown'}\n\n${trailers.join('\n')}`;
    const commitArgs = ['commit', FORCE_BRANCH_FLAG, '.', '--', '-m', commitMessage];
    const { stdout } = await execFileAsync(gitQueuePath, commitArgs, { env, timeout: 60_000, cwd: repoRoot });
    const shaMatch = stdout.match(/\[\S+\s+([a-f0-9]+)\]/);
    if (shaMatch) sha = shaMatch[1];
    stepEmit('commit', 'completed', { sha, idempotent: false });
  } catch (err) {
    const stderr = extractStderr(err);
    // "nothing to commit" is a successful idempotent path — already committed
    if (!/nothing to commit|no changes added/i.test(stderr)) {
      const reason = classifyCommitFailure(stderr);
      emit(CHORUS_ACP_REFUSED, { role, card_id: cardId, step: 'commit', reason, detail: stderr.slice(0, 500) });
      throw new Error(`chorus_acp refused: ${reason} — ${stderr.split('\n')[0]}`);
    }
    stepEmit('commit', 'completed', { idempotent: true, reason: 'nothing-to-commit' });
  }

  // Push (idempotent; fast no-op if already pushed)
  // #2799 — pass --force-with-lease for the post-rebase push step.
  // chorus_acp's pre-push internal rebase moves local SHAs forward of
  // origin's pre-rebase ref; force-with-lease is safe (refuses on
  // concurrent peer push). Same flag chorus_commit passes (line ~858).
  stepEmit(STEP_PUSH, 'started', { branch });
  try {
    const pushArgs = branch
      ? [STEP_PUSH, FORCE_BRANCH_FLAG, FORCE_WITH_LEASE_FLAG, '--branch', branch]
      : [STEP_PUSH, FORCE_BRANCH_FLAG, FORCE_WITH_LEASE_FLAG];
    await execFileAsync(gitQueuePath, pushArgs, { env, timeout: 60_000, cwd: repoRoot });
    stepEmit(STEP_PUSH, 'completed');
  } catch (err) {
    const stderr = extractStderr(err);
    const reason = classifyPushFailure(stderr);
    emit(CHORUS_ACP_REFUSED, { role, card_id: cardId, step: STEP_PUSH, reason, detail: stderr.slice(0, 500) });
    throw new Error(`chorus_acp refused: ${reason} — ${stderr.split('\n')[0]}`);
  }

  // Step 3 — gh pr view (detect usable existing PR) → gh pr create (if missing
  // or stale) → gh pr merge.
  stepEmit('pr-view', 'started', { branch });
  let prUrl = '';
  let prAlreadyExists = false;
  try {
    const { stdout } = await execFileAsync('gh', ['pr', 'view', branch, '--json', 'url,state'], { env, cwd: repoRoot, timeout: 15_000 });
    const pr = JSON.parse(stdout) as { url: string; state: string };
    if (pr.state === 'OPEN') {
      prUrl = pr.url;
      prAlreadyExists = true;
      stepEmit('pr-view', 'completed', { pr_url: prUrl, exists: true, state: pr.state });
    } else {
      // #2913: `gh pr view <branch>` resolves to the *most recent* PR for the
      // branch name. On a reused branch — the card's branch already shipped a
      // merged PR, then accrued new commits — that's a stale MERGED/CLOSED PR,
      // not the PR for this work. The already-merged fast-path upstream already
      // confirmed there ARE unmerged commits, so this PR is definitively stale.
      // Treat it as "no usable PR" and fall through to pr-create — otherwise
      // gh pr merge hits the merged PR, "already merged" is caught as idempotent
      // success, and the new commits ship nothing (the #2913 self-acp failure).
      stepEmit('pr-view', 'completed', { exists: false, stale_pr_url: pr.url, stale_pr_state: pr.state });
    }
  } catch {
    // No PR for this branch at all.
    stepEmit('pr-view', 'completed', { exists: false });
  }

  if (!prAlreadyExists) {
    // No usable PR (none exists, or the only one is a stale merged/closed PR
    // on a reused branch) — create a fresh one for the unmerged commits.
    stepEmit('pr-create', 'started', { branch });
    try {
      const { stdout } = await execFileAsync(
        'gh',
        ['pr', 'create', '--title', `${role}: acp #${cardId ?? 'unknown'}`, '--body', `Automated /acp via chorus_acp MCP for #${cardId ?? 'unknown'}.`],
        { env, cwd: repoRoot, timeout: 30_000 },
      );
      prUrl = stdout.trim().split('\n').pop() ?? '';
      stepEmit('pr-create', 'completed', { pr_url: prUrl });
    } catch (err) {
      const stderr = extractStderr(err);
      emit(CHORUS_ACP_REFUSED, { role, card_id: cardId, step: 'pr-create', reason: 'pr-create-fail', detail: stderr.slice(0, 500) });
      throw new Error(`chorus_acp refused: pr-create-fail — ${stderr.split('\n')[0]}`);
    }
  }

  stepEmit('pr-merge', 'started', { pr_url: prUrl, pr_already_exists: prAlreadyExists });
  try {
    // #2753 — no --delete-branch flag: gh's branch deletion does an implicit
    // `git checkout main` which collides with canonical's worktree (dual-
    // checkout refusal). chorus-werk remove (called below) handles the
    // worktree + local branch + remote-ref cleanup correctly.
    // #2913 — merge by the resolved prUrl, not the branch name: on a reused
    // branch a stale merged PR and the fresh PR both share the branch name, so
    // `gh pr merge <branch>` is ambiguous. prUrl is unambiguously the PR we
    // resolved (OPEN existing) or just created.
    await execFileAsync('gh', ['pr', 'merge', prUrl, '--squash'], { env, cwd: repoRoot, timeout: 60_000 });
    stepEmit('pr-merge', 'completed', { idempotent: false });
  } catch (err) {
    const stderr = extractStderr(err);
    // "already merged" is idempotent success
    if (!/already.*merged|state.*MERGED/i.test(stderr)) {
      emit(CHORUS_ACP_REFUSED, { role, card_id: cardId, step: 'pr-merge', reason: 'pr-merge-fail', detail: stderr.slice(0, 500) });
      throw new Error(`chorus_acp refused: pr-merge-fail — ${stderr.split('\n')[0]}`);
    }
    stepEmit('pr-merge', 'completed', { idempotent: true, reason: ALREADY_MERGED });
  }

  // Step 4 — cards done.
  if (cardId !== null) {
    stepEmit('cards-done', 'started');
    try {
      await execFileAsync(cardsPath, ['done', String(cardId)], { env, timeout: 15_000 });
      stepEmit('cards-done', 'completed');
    } catch (err) {
      const stderr = extractStderr(err);
      emit(CHORUS_ACP_REFUSED, { role, card_id: cardId, step: 'cards-done', reason: 'cards-done-fail', detail: stderr.slice(0, 500) });
      throw new Error(`chorus_acp refused: cards-done-fail — ${stderr.split('\n')[0]}`);
    }
  }

  // Step 5 — spine event card.accepted.
  emit(CARD_ACCEPTED, { role, card: cardId });

  // Step 6 — chorus-werk remove (best-effort; doesn't fail the transaction).
  // #2943: when this step fails, emit BOTH the existing werk-close.completed
  // (with branch_closed: false for backward-compat observability) AND the
  // typed CHORUS_ACP_REFUSED with reason: 'branch-close-fail' so the taxonomy
  // contract documented on the tool description is honest. We do NOT throw —
  // card.accepted has already fired, the transaction is materially complete,
  // and an orphan remote branch is recoverable via the idempotent re-run
  // path (above) that detects the accepted-card + orphan-branch state and
  // runs werk-close alone. The refusal event is the typed signal for
  // dashboards/observability/operators; it does not break the contract.
  let branchClosed = false;
  if (cardId !== null) {
    stepEmit(STEP_WERK_CLOSE, 'started');
    try {
      const chorusWerkPath = path.join(repoRoot, 'platform', 'scripts', CHORUS_WERK);
      await execFileAsync(chorusWerkPath, ['remove', role, String(cardId)], { env, timeout: 30_000 });
      branchClosed = true;
      stepEmit(STEP_WERK_CLOSE, 'completed', { branch_closed: true });
    } catch (err) {
      // Non-fatal — branch close is hygiene; the transaction is already complete.
      const stderr = extractStderr(err);
      stepEmit(STEP_WERK_CLOSE, 'completed', { branch_closed: false, error: stderr.slice(0, 200) });
      // #2943 — typed refusal signal (non-throwing). Mirrors the shape of
      // other CHORUS_ACP_REFUSED emits so callers handling refusal taxonomy
      // see the case. To recover: re-run /acp; the idempotent path above
      // detects the orphan branch and runs werk-close alone.
      emit(CHORUS_ACP_REFUSED, {
        role,
        card_id: cardId,
        step: STEP_WERK_CLOSE,
        reason: 'branch-close-fail',
        detail: stderr.slice(0, 500),
        recoverable: true,
        recovery_hint: `re-run \`/acp ${cardId}\` to retry branch-close (idempotent on accepted cards)`,
      });
    }
  }

  // #2863 — release event: kick building-pipeline so /build runs immediately
  // against origin/main (whose first step fast-forwards canonical). Replaces
  // the chorus-werk-sync 10-min poll. Best-effort: kickstart failure does
  // not fail /acp because the merge already landed; flag in step event so
  // an operator can recover manually.
  stepEmit('release-trigger', 'started');
  try {
    await execFileAsync('launchctl', ['kickstart', `gui/${process.getuid?.() ?? 0}/com.chorus.building-pipeline`], { env, timeout: 5_000 });
    stepEmit('release-trigger', 'completed');
  } catch (err) {
    const stderr = extractStderr(err);
    stepEmit('release-trigger', 'completed', { ok: false, error: stderr.slice(0, 200) });
  }

  // #2897: cleanup demo trace file on /acp success (normal path)
  try { const fsp = await import('fs/promises'); await fsp.unlink(`/tmp/demo-trace-${cardId}.txt`); } catch { /* file may not exist if /demo wasn't invoked this session */ }
  emit('chorus_acp.completed', { role, card_id: cardId, sha, pr_url: prUrl, branch_closed: branchClosed });

  return {
    content: [
      {
        type: 'text',
        text: JSON.stringify({ role, card_id: cardId, sha, pr_url: prUrl, branch_closed: branchClosed }, null, 2),
      },
    ],
  };
}

// #2751 — chorus_pull_card atomic transaction. Mirrors executeAcp's shape:
// inject deps, run the steps deterministically, emit step-by-step spine
// events, refuse with typed reasons that name the failing step and what
// the operator must fix.
async function executePullCard(
  args: { role: 'kade' | 'wren' | 'silas'; card_id: number },
  emit: SpineEmitter,
  execFileAsync: ExecFileAsync,
  cardsPath: string,
  resolveWorkingTree: (role: 'kade' | 'wren' | 'silas') => string,
): Promise<{ content: Array<{ type: 'text'; text: string }> }> {
  const { role, card_id: cardId } = args;
  // #2913: at pull time the card's ephemeral werk does not exist yet —
  // `chorus-werk add` creates it below. resolveWorkingTree finds no
  // <role>-<card> match and returns canonical; that is the correct root
  // for invoking the substrate scripts (chorus-werk, role-state).
  const repoRoot = resolveWorkingTree(role);
  const branch = `${role}/${cardId}`;

  // #2857 — flow trace. card_id is known at entry (from args), so wrap once.
  const baseEmit = emit;
  const trace_id = mintTraceIdV7();
  emit = createSpineEmitter(trace_id, baseEmit, cardId);

  // #2931: per-step duration_ms — same shape as chorus_acp stepEmit. Map is
  // closure-local; key is step name; only completed events carry duration.
  const stepStartedAt = new Map<string, number>();
  const stepEmit = (step: string, status: 'started' | 'completed', detail?: Record<string, unknown>) => {
    if (status === 'started') stepStartedAt.set(step, Date.now());
    const dur = status === 'completed' ? stepStartedAt.get(step) : undefined;
    const duration = dur !== undefined ? { duration_ms: Date.now() - dur } : {};
    emit(`chorus_pull_card.${step}.${status}`, { role, card_id: cardId, ...duration, ...(detail ?? {}) });
  };

  emit('chorus_pull_card.invoked', { role, card_id: cardId, repo_root: repoRoot });

  const path = require('path') as typeof import('path');
  const parentNodeBinDir = path.dirname(process.execPath);
  const cargoBinDir = `${process.env.HOME ?? ''}/.cargo/bin`;
  const env = {
    ...process.env,
    DEPLOY_ROLE: role,
    CHORUS_TRACE_ID: trace_id,
    CHORUS_CARD_ID: String(cardId),
    PATH: `${parentNodeBinDir}:${cargoBinDir}:${process.env.PATH ?? ''}`,
  } as NodeJS.ProcessEnv;

  const refuse = (step: string, reason: string, detail: string): never => {
    emit(CHORUS_PULL_CARD_REFUSED, { role, card_id: cardId, step, reason, detail: detail.slice(0, 500) });
    throw new Error(`chorus_pull_card refused: ${reason} — ${detail.split('\n')[0]}`);
  };

  // Step 1 — validate card via cards CLI (read-only).
  stepEmit('validate', 'started');
  // Real `cards view --json` returns: index/title/owner/status/priority/description/domains/comments/created/updated.
  // (Field is `description`, not `desc`. The mock in mcp-pull-card.test.ts uses `desc` for brevity; the
  // executor accepts either to keep tests honest.)
  let cardJson: { id?: number; index?: number; status?: string; owner?: string; desc?: string; description?: string } = {};
  try {
    const { stdout } = await execFileAsync(cardsPath, ['view', String(cardId), '--json'], { env, timeout: 10_000 });
    cardJson = JSON.parse(stdout) as typeof cardJson;
  } catch (err) {
    refuse('validate', 'card-not-found', extractStderr(err) || `card ${cardId} not viewable`);
  }
  const status = cardJson.status ?? '';
  if (status !== 'Next' && status !== 'Later') {
    refuse('validate', 'wrong-status', `card #${cardId} is in '${status}' — must be Next or Later`);
  }
  const desc = cardJson.description ?? cardJson.desc ?? '';
  if (!/^\s*-\s*\[[ x]\]/m.test(desc)) {
    refuse('validate', 'ac-missing', `card #${cardId} description has no AC checklist (no '- [ ]' or '- [x]' line)`);
  }
  if (!/##\s*Experience/im.test(desc)) {
    refuse('validate', 'experience-missing', `card #${cardId} description has no '## Experience' section`);
  }
  stepEmit('validate', 'completed', { status });

  // #2913: no werk pre-flight. Under the persistent-werk model this step
  // checked one stable chorus-werk/<role>/ dir for dirty/wrong-branch state
  // carried over from a prior card. The ephemeral model has no carry-over —
  // each card gets a fresh worktree, created below by `chorus-werk add`,
  // which is idempotent and refuses (exit 3) if a dir already exists on a
  // different branch. The protection moved into `add` itself; pre-flighting
  // a werk that does not exist yet was checking the wrong thing (and would
  // false-refuse if canonical happened to be dirty).

  // Step 2 — cards move WIP. Idempotent on already-WIP via cards CLI's own check.
  stepEmit(STEP_CARDS_MOVE, 'started');
  try {
    await execFileAsync(cardsPath, ['move', String(cardId), 'WIP'], { env, timeout: 15_000 });
    stepEmit(STEP_CARDS_MOVE, 'completed');
  } catch (err) {
    const stderr = extractStderr(err);
    if (!/already.*WIP|already in WIP/i.test(stderr)) {
      refuse(STEP_CARDS_MOVE, 'move-fail', stderr);
    }
    stepEmit(STEP_CARDS_MOVE, 'completed', { idempotent: true });
  }

  // Step 3 — chorus-werk add: create the card's ephemeral worktree at
  // chorus-werk/<role>-<card>/ on branch <role>/<card-id>, branched from
  // origin/main. Idempotent — a re-pull of the same card is a no-op.
  stepEmit('werk-add', 'started', { branch });
  try {
    const chorusWerkPath = path.join(repoRoot, 'platform', 'scripts', CHORUS_WERK);
    await execFileAsync(chorusWerkPath, ['add', role, String(cardId)], { env, timeout: 30_000 });
    stepEmit('werk-add', 'completed', { branch });
  } catch (err) {
    refuse('werk-add', 'branch-fail', extractStderr(err));
  }

  // Step 4 — role-state declare building.
  stepEmit(STEP_ROLE_STATE, 'started');
  try {
    const roleStatePath = path.join(repoRoot, 'platform', 'scripts', STEP_ROLE_STATE);
    await execFileAsync(roleStatePath, [role, 'building'], { env, timeout: 10_000 });
    stepEmit(STEP_ROLE_STATE, 'completed');
  } catch {
    // Non-fatal — board state is already updated; role-state is a session-attention hint.
    stepEmit(STEP_ROLE_STATE, 'completed', { warning: 'role-state declare failed (non-fatal)' });
  }

  // Step 5 — spine event card.pulled.
  emit(CARD_PULLED, { role, card_id: cardId, branch });
  emit('chorus_pull_card.completed', { role, card_id: cardId, branch });

  return {
    content: [
      { type: 'text', text: JSON.stringify({ role, card_id: cardId, branch }, null, 2) },
    ],
  };
}

// #2759 — chorus_unpull_card atomic teardown. /pull's natural inverse.
// Role + card_id; refuses if card isn't WIP-owned-by-role or werk has
// uncommitted work. Uses chorus-werk remove to tear down the card's
// ephemeral worktree + branch + emit card.branch.closed (#2913).
// cog-override: unpull teardown handles multiple failure modes across werk + board + branch — structurally complex
async function executeUnpullCard(
  args: { role: 'kade' | 'wren' | 'silas'; card_id: number },
  emit: SpineEmitter,
  execFileAsync: ExecFileAsync,
  cardsPath: string,
  resolveWorkingTree: (role: 'kade' | 'wren' | 'silas') => string,
  fsExists: (p: string) => boolean,
): Promise<{ content: Array<{ type: 'text'; text: string }> }> {
  const { role, card_id: cardId } = args;
  const repoRoot = resolveWorkingTree(role);
  const branch = `${role}/${cardId}`;

  // #2857 — flow trace. card_id is known at entry (from args), so wrap once.
  const baseEmit = emit;
  const trace_id = mintTraceIdV7();
  emit = createSpineEmitter(trace_id, baseEmit, cardId);

  const stepEmit = (step: string, status: 'started' | 'completed', detail?: Record<string, unknown>) =>
    emit(`chorus_unpull_card.${step}.${status}`, { role, card_id: cardId, ...(detail ?? {}) });

  emit('chorus_unpull_card.invoked', { role, card_id: cardId, repo_root: repoRoot });

  const path = require('path') as typeof import('path');
  const parentNodeBinDir = path.dirname(process.execPath);
  const cargoBinDir = `${process.env.HOME ?? ''}/.cargo/bin`;
  const env = {
    ...process.env,
    DEPLOY_ROLE: role,
    CHORUS_TRACE_ID: trace_id,
    CHORUS_CARD_ID: String(cardId),
    PATH: `${parentNodeBinDir}:${cargoBinDir}:${process.env.PATH ?? ''}`,
  } as NodeJS.ProcessEnv;

  const refuse = (step: string, reason: string, detail: string): never => {
    emit(CHORUS_UNPULL_CARD_REFUSED, { role, card_id: cardId, step, reason, detail: detail.slice(0, 500) });
    throw new Error(`chorus_unpull_card refused: ${reason} — ${detail.split('\n')[0]}`);
  };

  // Step 1 — validate: card exists, is WIP, owned by role.
  stepEmit('validate', 'started');
  let cardJson: { status?: string; owner?: string } = {};
  try {
    const { stdout } = await execFileAsync(cardsPath, ['view', String(cardId), '--json'], { env, timeout: 10_000 });
    cardJson = JSON.parse(stdout) as typeof cardJson;
  } catch (err) {
    refuse('validate', 'card-not-found', extractStderr(err) || `card ${cardId} not viewable`);
  }
  const status = cardJson.status ?? '';
  if (status !== 'WIP') {
    refuse('validate', 'wrong-status', `card #${cardId} is in '${status}' — must be WIP`);
  }
  // Owner field is title-cased ('Kade'); compare case-insensitively.
  const owner = (cardJson.owner ?? '').toLowerCase();
  if (owner !== role) {
    refuse('validate', 'wrong-owner', `card #${cardId} is owned by '${cardJson.owner}' — must be ${role}`);
  }
  stepEmit('validate', 'completed', { status, owner });

  // Step 2 — werk pre-flight. Unlike pull, the card's ephemeral werk DOES
  // exist at unpull time (it was created when the card was pulled), so
  // pre-flighting it is checking the right thing. Refuse on a missing werk
  // path, exec failures (werk-corrupt), or uncommitted changes (don't lose
  // work — `chorus-werk remove` also refuses dirty, this surfaces it earlier
  // with a typed reason). #2913: resolveWorkingTree returns the single
  // chorus-werk/<role>-<card>/ match when the role has one card in flight.
  stepEmit(STEP_WERK_PREFLIGHT, 'started');
  if (!fsExists(repoRoot)) {
    refuse(STEP_WERK_PREFLIGHT, 'werk-not-initialized', `werk path does not exist: ${repoRoot} — the card's werk may already be removed`);
  }
  if (!fsExists(path.join(repoRoot, '.git'))) {
    refuse(STEP_WERK_PREFLIGHT, 'werk-not-initialized', `werk path exists but is not a git worktree: ${repoRoot}`);
  }
  let dirty = '';
  try {
    const r = await execFileAsync('git', ['status', '--porcelain'], { env, cwd: repoRoot, timeout: 5_000 });
    dirty = r.stdout;
  } catch (err) {
    refuse(STEP_WERK_PREFLIGHT, 'werk-corrupt', `git status failed at ${repoRoot}: ${extractStderr(err)}`);
  }
  if (dirty.trim().length > 0) {
    refuse(STEP_WERK_PREFLIGHT, 'werk-dirty', `werk has uncommitted changes:\n${dirty.trim()}`);
  }
  stepEmit(STEP_WERK_PREFLIGHT, 'completed');

  // Step 3 — cards move <id> Next.
  stepEmit(STEP_CARDS_MOVE, 'started');
  try {
    await execFileAsync(cardsPath, ['move', String(cardId), 'Next'], { env, timeout: 15_000 });
    stepEmit(STEP_CARDS_MOVE, 'completed');
  } catch (err) {
    const stderr = extractStderr(err);
    if (!/already.*Next|already in Next/i.test(stderr)) {
      refuse(STEP_CARDS_MOVE, 'move-fail', stderr);
    }
    stepEmit(STEP_CARDS_MOVE, 'completed', { idempotent: true });
  }

  // Step 4 — chorus-werk remove <role> <card_id>. #2913 ephemeral model:
  // remove the card's worktree, delete the local branch, best-effort delete
  // the remote, prune stale admin entries, emit card.branch.closed. `remove`
  // refuses on a dirty werk — the Step 2 pre-flight already caught that, this
  // is belt-and-suspenders. There is no done-state check on `remove` (the
  // old `close --no-done-check` flag is gone): remove is not gated on card
  // status, it just refuses to drop uncommitted work.
  stepEmit(STEP_WERK_CLOSE, 'started');
  let branchClosed = false;
  try {
    const chorusWerkPath = path.join(repoRoot, 'platform', 'scripts', CHORUS_WERK);
    await execFileAsync(chorusWerkPath, ['remove', role, String(cardId)], { env, timeout: 30_000 });
    branchClosed = true;
    stepEmit(STEP_WERK_CLOSE, 'completed', { branch_closed: true });
  } catch (err) {
    const stderr = extractStderr(err);
    if (/already removed/i.test(stderr)) {
      // Idempotent: worktree + branch were already torn down.
      branchClosed = true;
      stepEmit(STEP_WERK_CLOSE, 'completed', { idempotent: true });
    } else {
      refuse(STEP_WERK_CLOSE, 'branch-close-fail', stderr);
    }
  }

  // Step 5 — role-state idle (best-effort; non-fatal).
  stepEmit(STEP_ROLE_STATE, 'started');
  try {
    const roleStatePath = path.join(repoRoot, 'platform', 'scripts', STEP_ROLE_STATE);
    await execFileAsync(roleStatePath, [role, 'idle'], { env, timeout: 10_000 });
    stepEmit(STEP_ROLE_STATE, 'completed');
  } catch {
    stepEmit(STEP_ROLE_STATE, 'completed', { warning: 'role-state idle failed (non-fatal)' });
  }

  // Step 6 — spine event card.unpulled.
  emit(CARD_UNPULLED, { role, card_id: cardId, prior_branch: branch });
  emit('chorus_unpull_card.completed', { role, card_id: cardId, prior_branch: branch, branch_closed: branchClosed });

  return {
    content: [
      { type: 'text', text: JSON.stringify({ role, card_id: cardId, prior_branch: branch, branch_closed: branchClosed }, null, 2) },
    ],
  };
}

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

// #2969 — chorus_doc_catalog_add: POST to /api/doc-catalog/add, which delegates
// to registerDoc() in handlers/doc-catalog.ts. Same code path the HTTP endpoint
// uses; this is the typed agent-facing surface. Refusal taxonomy mirrors
// registerDoc's HTTP status codes: 400/404/409.
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
  let stdout = '';
  let stderr = '';
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

async function executeNudge(
  args: NudgeArgs,
  from: string,
  fetchImpl: FetchImpl,
  pulseUrl: string,
): Promise<{ content: Array<{ type: 'text'; text: string }> }> {
  const { to, message } = args;
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
      body: JSON.stringify({ from, to, content: message, traceId }),
      signal: ctrl.signal,
    });
    clearTimeout(timeoutId);
    if (!resp.ok) {
      const errText = resp.text ? await resp.text().catch(() => '') : '';
      throw new Error(`pulse POST returned ${resp.status}: ${errText.slice(0, 200)}`);
    }
    logEvent('info', 'mcp.nudge.delivered', { from, to, trace_id: traceId });
    return { content: [{ type: 'text', text: `nudge sent: ${from} → ${to} (trace=${traceId})` }] };
  } catch (err) {
    const errMsg = err instanceof Error ? err.message : String(err);
    logEvent('error', 'mcp.nudge.failed', { from, to, trace_id: traceId, error: errMsg });
    throw new Error(`nudge delivery failed: ${errMsg}`);
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
    const errMsg = err instanceof Error ? err.message : String(err);
    logEvent('error', `mcp.cards.${verb}.failed`, { from, error: errMsg });
    throw new Error(`${toolName} failed: ${errMsg}`);
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

async function executeCardsMove(
  args: z.infer<typeof CardsMoveInput>,
  from: string,
  execFileAsync: ExecFileAsync,
  cardsPath: string,
): Promise<{ content: Array<{ type: 'text'; text: string }> }> {
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
  // Sequence uses the dedicated `sequence-tag` verb; domain/chunk use `label add/remove`.
  let verb: string;
  let argv: string[];
  const op = args.op ?? 'add';
  if (args.category === 'sequence') {
    if (op !== 'add') {
      // Removal of a sequence tag goes through generic untag.
      verb = 'untag';
      argv = [String(args.id), `sequence:${args.value}`];
    } else {
      verb = 'sequence-tag';
      argv = [String(args.id), args.value];
    }
  } else {
    verb = op === 'add' ? 'tag' : 'untag';
    argv = [String(args.id), `${args.category}:${args.value}`];
  }
  const out = await execCardsCli(verb, argv, from, execFileAsync, cardsPath, 'chorus_cards_tag');
  return { content: [{ type: 'text', text: out }] };
}

async function executeCardsSet(
  args: z.infer<typeof CardsSetInput>,
  from: string,
  execFileAsync: ExecFileAsync,
  cardsPath: string,
): Promise<{ content: Array<{ type: 'text'; text: string }> }> {
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
  const gitQueuePath = deps.gitQueuePath ?? resolveGitQueuePath();
  // #2913 — ephemeral-worktree cwd resolver. Default globs chorus-werk/<role>-*;
  // a single match is the role's active card werk, zero/ambiguous falls back to
  // canonical (#2662 cwd=repo-root contract preserved). No CHORUS_WERK_ENABLE
  // flag — the ephemeral model is the model, not an opt-in.
  const canonicalRepoRoot = gitQueuePath.replace(/\/platform\/scripts\/git-queue\.sh$/, '');
  const resolveWorkingTree: (role: 'kade' | 'wren' | 'silas') => string =
    deps.resolveWorkingTree ?? defaultResolveWorkingTree(canonicalRepoRoot);
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  const _fs = require('fs') as typeof import('fs');
  const fsExists: (p: string) => boolean = deps.fsExists ?? ((p: string) => _fs.existsSync(p));
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
      PRINCIPLES_LIST_TOOL_DEF,
      PRINCIPLES_GET_TOOL_DEF,
      PRINCIPLES_CREATE_TOOL_DEF,
      DECISIONS_LIST_TOOL_DEF,
      DECISIONS_GET_TOOL_DEF,
      SUBDOMAINS_LIST_TOOL_DEF,
      SUBDOMAINS_GET_TOOL_DEF,
      CARDS_ADD_TOOL_DEF,
      CARDS_MOVE_TOOL_DEF,
      CARDS_DONE_TOOL_DEF,
      CARDS_TAG_TOOL_DEF,
      CARDS_SET_TOOL_DEF,
      CARDS_VIEW_TOOL_DEF,
      COMMIT_STATUS_TOOL_DEF,
      COMMIT_TOOL_DEF,
      PULL_TOOL_DEF,
      PULL_CARD_TOOL_DEF,
      UNPULL_CARD_TOOL_DEF,
      ACP_TOOL_DEF,
      DESIGN_REFRESH_TOOL_DEF,
      DOC_CATALOG_ADD_TOOL_DEF,
      LOGS_QUERY_TOOL_DEF,
      LOGS_RECENT_ERRORS_TOOL_DEF,
      LOGS_FOR_CARD_TOOL_DEF,
      LOGS_FOR_TRACE_TOOL_DEF,
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
    switch (req.params.name) {
      case 'chorus_nudge_message': {
        const parsed = NudgeInput.safeParse(req.params.arguments);
        if (!parsed.success) {
          throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
        }
        // #2804 — executeNudge POSTs to pulse instead of spawning shim.
        const pulseUrl = process.env.CHORUS_PULSE_URL || 'http://localhost:3475/api/nudge';
        return executeNudge(parsed.data, from, fetchImpl, pulseUrl);
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
      case 'chorus_subdomains_list':
        return executeSubdomainsList(fetchImpl, apiBase, from);
      case 'chorus_subdomains_get': {
        const parsed = SubdomainsGetInput.safeParse(req.params.arguments);
        if (!parsed.success) {
          throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
        }
        return executeSubdomainsGet(parsed.data, fetchImpl, apiBase, from);
      }
      case 'chorus_cards_add': {
        const parsed = CardsAddInput.safeParse(req.params.arguments);
        if (!parsed.success) {
          throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
        }
        return executeCardsAdd(parsed.data, from, execFileAsync, cardsPath);
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
      case 'chorus_commit_status': {
        const parsed = CommitStatusInput.safeParse(req.params.arguments);
        if (!parsed.success) {
          throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
        }
        return executeCommitStatus(parsed.data, boardReader, emitSpineEvent);
      }
      case 'chorus_commit': {
        const parsed = CommitInput.safeParse(req.params.arguments);
        if (!parsed.success) {
          throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
        }
        return executeCommit(parsed.data, boardReader, emitSpineEvent, execFileAsync, gitQueuePath, resolveWorkingTree);
      }
      case 'chorus_pull': {
        const parsed = PullInput.safeParse(req.params.arguments);
        if (!parsed.success) {
          throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
        }
        return executePull(parsed.data, boardReader, emitSpineEvent, execFileAsync, gitQueuePath, resolveWorkingTree);
      }
      case 'chorus_acp': {
        const parsed = AcpInput.safeParse(req.params.arguments);
        if (!parsed.success) {
          throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
        }
        return executeAcp(parsed.data, boardReader, emitSpineEvent, execFileAsync, gitQueuePath, cardsPath, resolveWorkingTree);
      }
      case 'chorus_pull_card': {
        const parsed = PullCardInput.safeParse(req.params.arguments);
        if (!parsed.success) {
          throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
        }
        return executePullCard(parsed.data, emitSpineEvent, execFileAsync, cardsPath, resolveWorkingTree);
      }
      case 'chorus_unpull_card': {
        const parsed = UnpullCardInput.safeParse(req.params.arguments);
        if (!parsed.success) {
          throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
        }
        return executeUnpullCard(parsed.data, emitSpineEvent, execFileAsync, cardsPath, resolveWorkingTree, fsExists);
      }
      case 'chorus_design_refresh': {
        const parsed = DesignRefreshInput.safeParse(req.params.arguments);
        if (!parsed.success) {
          throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
        }
        try {
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
        } catch (err) {
          // executeDesignRefresh emits design.refresh.failed itself on typed
          // refusal; rethrow so the MCP surface returns an error response.
          throw err;
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
      case 'chorus_logs_for_trace': {
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
      case 'chorus_tree_get': {
        const parsed = TreeGetInput.safeParse(req.params.arguments ?? {});
        if (!parsed.success) {
          throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
        }
        try {
          const tree = athenaLoadTree();
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
    }
  });

  return server;
}
