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
import { resolveCardsPath } from '../cards-path';
import { resolveGitQueuePath } from '../git-queue-path';

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
  // #2750 — resolve the working tree (cwd) for git-queue.sh subprocs. When a
  // role's CHORUS_WERK_ENABLE flag is on, commits and pulls must run against
  // /chorus-werk/<role>/, not canonical (#2735). Default impl reads role's
  // settings.json env block; tests inject a stub path. Returning the
  // canonical repo root preserves pre-#2750 behavior exactly (#2662
  // cwd=repo-root contract).
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

// #2750 slice 2 — chorus_acp atomic transaction input. Single role; card derived from board.
const AcpInput = z.object({
  role: z.enum(['kade', 'wren', 'silas']).describe('Calling role — kade/wren/silas. Card derived from board (status=WIP, owner=role).'),
}).strict();

// #2751 — chorus_pull_card atomic transaction input. Role + explicit card_id;
// the /pull skill is the caller, and Jeff or the role names which card.
// No bypasses on the wire — werk-dirty / werk-wrong-branch are typed refusals,
// not flags the caller can suppress.
const PullCardInput = z.object({
  role: z.enum(['kade', 'wren', 'silas']).describe('Calling role — kade/wren/silas. DEPLOY_ROLE attribution + spine event role field.'),
  card_id: z.number().int().positive().describe('Card ID to pull. Must be in Next or Later status with AC + Experience populated.'),
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

// #2750 slice 2 — chorus_acp tool def. Atomic /acp transaction:
// commit + push + PR open (or detect existing) + PR merge (squash + delete-branch)
// + cards done + card.accepted spine event + chorus-werk close (when flag on).
// Skill collapses to a single MCP call so model-compliance gaps can't shortcut steps.
const UNPULL_CARD_TOOL_DEF = {
  name: 'chorus_unpull_card',
  description:
    'Use this to reverse a pull and tear down the role\'s WIP card cleanly. Service runs validate (must be WIP + owned by role) + werk pre-flight (refuses werk-dirty) + cards move <id> Next + chorus-werk close (detach + branch teardown + werk.detached event) + role-state idle + card.unpulled spine event in one atomic transaction. Returns { role, card_id, prior_branch, branch_closed }. Refusal taxonomy: card-not-found | wrong-status | wrong-owner | werk-dirty | move-fail | branch-close-fail. Do NOT use raw cards/git/role-state — those bypass the typed refusal taxonomy and leave stale branches. The /unpull skill calls this and nothing else.',
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
    'Use this to pull a card to WIP and ready the role\'s werk for building. Service runs validate + werk-pre-flight (refuses werk-dirty / werk-wrong-branch) + cards move WIP + chorus-werk repoint + role-state building + card.pulled spine event in one atomic transaction. Returns { role, card_id, branch }. Refusal taxonomy: card-not-found | wrong-status | ac-missing | experience-missing | werk-dirty | werk-wrong-branch | move-fail | branch-fail. Do NOT use raw cards/git/role-state — those bypass the typed refusal taxonomy. The /pull skill calls this and nothing else.',
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

const ACP_TOOL_DEF = {
  name: 'chorus_acp',
  description:
    'Use this to accept the current WIP card end-to-end. Service runs verify-after sequenced steps with typed refusal at each step: derive card_id from HEAD branch (`<role>/<card-id>`), commit + push, PR open/merge, cards-done, spine event, branch-close. Idempotent on re-run (detects existing PR / closed branch / already-merged work). Returns { role, card_id, sha, pr_url, branch_closed }. Refusal taxonomy: hook-fail | commit-fail | push-conflict | pr-create-fail | pr-merge-fail | cards-done-fail | branch-close-fail. Each step that fails refuses by name; success means every step verified. Do NOT use raw git, gh, or cards CLI — those bypass the typed refusal taxonomy. The /acp skill calls this and nothing else.',
  inputSchema: {
    type: 'object',
    properties: {
      role: {
        type: 'string',
        enum: ['kade', 'wren', 'silas'],
        description: 'Calling role — kade / wren / silas. Card derived from board (status=WIP, owner=role).',
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
const CHORUS_UNPULL_CARD_REFUSED = 'chorus_unpull_card.refused';
const CARD_UNPULLED = 'card.unpulled';
// #2752 — sonarjs no-duplicate-string: extract literals appearing >5x
const FORCE_BRANCH_FLAG = '--force-branch';
const ALREADY_MERGED = 'already-merged';
const STEP_PUSH = 'push';

interface AcpArgs {
  role: 'kade' | 'wren' | 'silas';
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

// #2750 — default werk-aware resolver. Reads role's settings.json env block
// for CHORUS_WERK_ENABLE. If "1", returns the role's werk path (sibling to
// canonical: $HOME/CascadeProjects/chorus-werk/<role>). Else returns canonical.
//
// #2779 / latent L1: NO caching. The cache shipped with #2750 was claimed safe
// because "settings.json is a small file, role-state changes are rare events
// bounded by session lifecycle." Both halves wrong: settings.json mutates
// during a session (env-block edits, read-tree reset to main, role
// reassignments), and "rare" is not "never" — when it happened on 2026-05-07
// the cached resolution silently routed commits to canonical for the daemon's
// lifetime. Cost: ~30 minutes of forensic work + a parked-branch recovery.
// The fix is to read settings.json on every call. The file is ~1KB; JSON.parse
// is microseconds; this is per-MCP-request, not per-spine-event. Correctness
// is worth the read.
export function defaultResolveWorkingTree(canonicalRoot: string): (role: 'kade' | 'wren' | 'silas') => string {
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  const fs = require('node:fs') as typeof import('node:fs');
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  const path = require('node:path') as typeof import('node:path');

  return (role: 'kade' | 'wren' | 'silas'): string => {
    const settingsPath = path.join(canonicalRoot, 'roles', role, '.claude', 'settings.json');
    let werkEnabled = false;
    try {
      const raw = fs.readFileSync(settingsPath, 'utf8');
      const parsed = JSON.parse(raw) as { env?: Record<string, string> };
      werkEnabled = parsed.env?.CHORUS_WERK_ENABLE === '1';
    } catch {
      // settings.json missing or unreadable — treat as flag-off, fall back to canonical
      werkEnabled = false;
    }

    if (werkEnabled) {
      // CHORUS_WERK_BASE convention: sibling of canonical, parent dir + /chorus-werk/
      const parent = path.dirname(canonicalRoot);
      return path.join(parent, 'chorus-werk', role);
    }
    return canonicalRoot;
  };
}

async function executeCommit(
  args: CommitArgs,
  boardReader: BoardReader,
  emit: SpineEmitter,
  execFileAsync: ExecFileAsync,
  gitQueuePath: string,
  resolveWorkingTree: (role: 'kade' | 'wren' | 'silas') => string,
): Promise<{ content: Array<{ type: 'text'; text: string }> }> {
  const { role, paths, message, no_add } = args;

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
  const pushArgs = pushRef
    ? [STEP_PUSH, FORCE_BRANCH_FLAG, '--branch', pushRef]
    : [STEP_PUSH, FORCE_BRANCH_FLAG];
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
  const repoRoot = resolveWorkingTree(role);
  // #2752 bug-4 — step-by-step logging. Each step emits .started before the
  // subprocess and .completed after success. Refusals already named the
  // step. Now any failure mode shows the exact step that ran/failed without
  // re-running with verbose flags.
  let cardId: number | null = null;
  const stepEmit = (step: string, status: 'started' | 'completed', detail?: Record<string, unknown>) =>
    emit(`chorus_acp.${step}.${status}`, { role, card_id: cardId, ...(detail ?? {}) });

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
  stepEmit('board-lookup', 'completed', { card_id: cardId, board_ok: board.ok, source: branchMatch ? 'branch' : 'board' });

  // #2752 bug-5 — transaction-level idempotency. If the PR is already merged
  // to origin/main, the commit+push+merge phase is moot — skip to cards-done
  // + werk-close. This catches Silas's silas/2177 case: PR squash-merged on
  // first attempt, subsequent retries kept failing on push (origin's branch
  // ref is stale pre-merge SHA, local has post-squash history, non-ff). The
  // right answer when work is already on main: just close the card.
  const transactionPath = require('path') as typeof import('path');
  const transactionEnv = {
    ...process.env,
    DEPLOY_ROLE: role,
    PATH: `${transactionPath.dirname(process.execPath)}:${process.env.HOME ?? ''}/.cargo/bin:${process.env.PATH ?? ''}`,
  } as NodeJS.ProcessEnv;
  let alreadyMerged = false;
  try {
    const { stdout: branchOut } = await execFileAsync('git', ['rev-parse', '--abbrev-ref', 'HEAD'], { env: transactionEnv, cwd: repoRoot, timeout: 5_000 });
    const currentBranch = branchOut.trim();
    if (currentBranch && currentBranch !== 'main') {
      // Check if the branch's tip (or any of its commits) is reachable from origin/main.
      // git merge-base --is-ancestor returns 0 if HEAD is an ancestor of origin/main.
      await execFileAsync('git', ['fetch', '--quiet', 'origin', 'main'], { env: transactionEnv, cwd: repoRoot, timeout: 15_000 });
      try {
        // Strategy: check if the role's branch ref on origin (which is the stale
        // pre-merge SHA, if PR already squashed) has been incorporated into origin/main
        // via squash-merge detection. Simpler: if `gh pr view <branch> --json state`
        // returns MERGED, the work is on main.
        const { stdout: prStateOut } = await execFileAsync(
          'gh',
          ['pr', 'view', currentBranch, '--json', 'state', '-q', '.state'],
          { env: transactionEnv, cwd: repoRoot, timeout: 15_000 },
        );
        if (prStateOut.trim() === 'MERGED') {
          alreadyMerged = true;
        }
      } catch {
        /* gh pr view failed (no PR for this branch yet) — proceed normally */
      }
    }
  } catch {
    /* fall through; normal commit+push path will handle */
  }
  stepEmit('already-merged-check', 'completed', { already_merged: alreadyMerged });

  if (alreadyMerged) {
    // Skip commit/push/PR-merge — jump straight to cards-done + werk-close.
    stepEmit('skip-to-closure', 'started', { reason: 'pr-already-merged-to-main' });
    if (cardId !== null) {
      try {
        await execFileAsync(cardsPath, ['done', String(cardId)], { env: transactionEnv, timeout: 15_000 });
      } catch (err) {
        const stderr = extractStderr(err);
        // "already Done" is success
        if (!/already.*Done|Card.*Done/i.test(stderr)) {
          emit(CHORUS_ACP_REFUSED, { role, card_id: cardId, step: 'cards-done', reason: 'cards-done-fail', detail: stderr.slice(0, 500) });
          throw new Error(`chorus_acp refused: cards-done-fail — ${stderr.split('\n')[0]}`);
        }
      }
    }
    emit(CARD_ACCEPTED, { role, card: cardId });
    let branchClosed = false;
    if (cardId !== null) {
      try {
        const chorusWerkPath = transactionPath.join(repoRoot, 'platform', 'scripts', 'chorus-werk');
        await execFileAsync(chorusWerkPath, ['close', role, String(cardId)], { env: transactionEnv, timeout: 30_000 });
        branchClosed = true;
      } catch {
        /* non-fatal */
      }
    }
    emit('chorus_acp.completed', { role, card_id: cardId, sha: ALREADY_MERGED, pr_url: ALREADY_MERGED, branch_closed: branchClosed, fast_path: true });
    return {
      content: [
        {
          type: 'text',
          text: JSON.stringify({ role, card_id: cardId, sha: ALREADY_MERGED, pr_url: ALREADY_MERGED, branch_closed: branchClosed, fast_path: true }, null, 2),
        },
      ],
    };
  }

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

  // #2793 — refuse werk-on-main with typed reason. gh pr create with
  // --head main --base main is unrecoverable in-place; pre-#2793 the call
  // marched on and produced 4 different ad-hoc gh-improvisations in one
  // session (2026-05-07: gh-merge-direct, cards-done-direct, force-push,
  // manual-PR). Refusing here gives operators one named recovery
  // (chorus-werk repoint) instead of inventing a path. Builds on #2782's
  // verify-after sequenced-steps shape: new typed refusal at a new step.
  if (branch === 'main') {
    const detail = cardId !== null
      ? `werk is on main; run: chorus-werk repoint ${role} ${role}/${cardId}`
      : `werk is on main and no card resolved from branch or board; pull a card first or repoint to ${role}/<card-id>`;
    emit(CHORUS_ACP_REFUSED, { role, card_id: cardId, step: 'pre-pr-create', reason: 'werk-on-main', detail });
    throw new Error(`chorus_acp refused: werk-on-main — ${detail}`);
  }

  // Commit (skip if no staged changes — gh push will be no-op).
  stepEmit('commit', 'started', { branch });
  try {
    const commitArgs = ['commit', FORCE_BRANCH_FLAG, '.', '--', '-m', `${role}: acp #${cardId ?? 'unknown'}`];
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
  stepEmit(STEP_PUSH, 'started', { branch });
  try {
    const pushArgs = branch ? [STEP_PUSH, FORCE_BRANCH_FLAG, '--branch', branch] : [STEP_PUSH, FORCE_BRANCH_FLAG];
    await execFileAsync(gitQueuePath, pushArgs, { env, timeout: 60_000, cwd: repoRoot });
    stepEmit(STEP_PUSH, 'completed');
  } catch (err) {
    const stderr = extractStderr(err);
    const reason = classifyPushFailure(stderr);
    emit(CHORUS_ACP_REFUSED, { role, card_id: cardId, step: STEP_PUSH, reason, detail: stderr.slice(0, 500) });
    throw new Error(`chorus_acp refused: ${reason} — ${stderr.split('\n')[0]}`);
  }

  // Step 3 — gh pr view (detect existing) → gh pr create (if missing) → gh pr merge.
  stepEmit('pr-view', 'started', { branch });
  let prUrl = '';
  let prAlreadyExists = false;
  try {
    const { stdout } = await execFileAsync('gh', ['pr', 'view', branch, '--json', 'url', '-q', '.url'], { env, cwd: repoRoot, timeout: 15_000 });
    prUrl = stdout.trim();
    prAlreadyExists = true;
    stepEmit('pr-view', 'completed', { pr_url: prUrl, exists: true });
  } catch {
    stepEmit('pr-view', 'completed', { exists: false });
    // No existing PR — create one.
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
    // checkout refusal). chorus-werk close (called below) handles local +
    // remote branch cleanup correctly via update-ref-d + push --delete.
    await execFileAsync('gh', ['pr', 'merge', branch, '--squash'], { env, cwd: repoRoot, timeout: 60_000 });
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

  // Step 6 — chorus-werk close (best-effort; doesn't fail the transaction).
  let branchClosed = false;
  if (cardId !== null) {
    stepEmit('werk-close', 'started');
    try {
      const chorusWerkPath = path.join(repoRoot, 'platform', 'scripts', 'chorus-werk');
      await execFileAsync(chorusWerkPath, ['close', role, String(cardId)], { env, timeout: 30_000 });
      branchClosed = true;
      stepEmit('werk-close', 'completed', { branch_closed: true });
    } catch (err) {
      // Non-fatal — branch close is hygiene; the transaction is already complete.
      const stderr = extractStderr(err);
      stepEmit('werk-close', 'completed', { branch_closed: false, error: stderr.slice(0, 200) });
    }
  }

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
  fsExists: (p: string) => boolean,
): Promise<{ content: Array<{ type: 'text'; text: string }> }> {
  const { role, card_id: cardId } = args;
  const repoRoot = resolveWorkingTree(role);
  const branch = `${role}/${cardId}`;

  const stepEmit = (step: string, status: 'started' | 'completed', detail?: Record<string, unknown>) =>
    emit(`chorus_pull_card.${step}.${status}`, { role, card_id: cardId, ...(detail ?? {}) });

  emit('chorus_pull_card.invoked', { role, card_id: cardId, repo_root: repoRoot });

  const path = require('path') as typeof import('path');
  const parentNodeBinDir = path.dirname(process.execPath);
  const cargoBinDir = `${process.env.HOME ?? ''}/.cargo/bin`;
  const env = {
    ...process.env,
    DEPLOY_ROLE: role,
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

  // Step 2 — werk pre-flight. Refuses werk-not-initialized, werk-dirty,
  // werk-corrupt, werk-wrong-branch with typed reasons.
  // #2751: dirty werk refusal stops contamination at the door.
  // #2760: existence check FIRST so a missing werk path doesn't get
  // mis-classified as werk-dirty. Wren hit this 2026-05-06: flag flipped,
  // init never ran. Separating exec failures from dirty-output checks
  // also kills the recursive double-throw the old try/catch produced.
  stepEmit('werk-preflight', 'started');
  if (!fsExists(repoRoot)) {
    refuse('werk-preflight', 'werk-not-initialized', `werk path does not exist: ${repoRoot} — run \`chorus-werk init ${role}\` first`);
  }
  if (!fsExists(path.join(repoRoot, '.git'))) {
    refuse('werk-preflight', 'werk-not-initialized', `werk path exists but is not a git worktree: ${repoRoot} — run \`chorus-werk init ${role}\` first`);
  }
  let dirty = '';
  try {
    const r = await execFileAsync('git', ['status', '--porcelain'], { env, cwd: repoRoot, timeout: 5_000 });
    dirty = r.stdout;
  } catch (err) {
    refuse('werk-preflight', 'werk-corrupt', `git status failed at ${repoRoot}: ${extractStderr(err)}`);
  }
  if (dirty.trim().length > 0) {
    refuse('werk-preflight', 'werk-dirty', `werk has uncommitted changes:\n${dirty.trim()}`);
  }
  let currentBranch = '';
  try {
    const r = await execFileAsync('git', ['rev-parse', '--abbrev-ref', 'HEAD'], { env, cwd: repoRoot, timeout: 5_000 });
    currentBranch = r.stdout.trim();
  } catch (err) {
    refuse('werk-preflight', 'werk-corrupt', `git rev-parse failed at ${repoRoot}: ${extractStderr(err)}`);
  }
  if (currentBranch !== 'main' && currentBranch !== 'HEAD' && currentBranch !== '') {
    refuse('werk-preflight', 'werk-wrong-branch', `werk is on '${currentBranch}' — must be on main or detached`);
  }
  stepEmit('werk-preflight', 'completed');

  // Step 3 — cards move WIP. Idempotent on already-WIP via cards CLI's own check.
  stepEmit('cards-move', 'started');
  try {
    await execFileAsync(cardsPath, ['move', String(cardId), 'WIP'], { env, timeout: 15_000 });
    stepEmit('cards-move', 'completed');
  } catch (err) {
    const stderr = extractStderr(err);
    if (!/already.*WIP|already in WIP/i.test(stderr)) {
      refuse('cards-move', 'move-fail', stderr);
    }
    stepEmit('cards-move', 'completed', { idempotent: true });
  }

  // Step 4 — chorus-werk repoint to <role>/<card-id>. Creates the branch
  // off origin/main if it doesn't exist; switches to it if it does.
  stepEmit('werk-repoint', 'started', { branch });
  try {
    const chorusWerkPath = path.join(repoRoot, 'platform', 'scripts', 'chorus-werk');
    await execFileAsync(chorusWerkPath, ['repoint', role, branch], { env, timeout: 30_000 });
    stepEmit('werk-repoint', 'completed', { branch });
  } catch (err) {
    refuse('werk-repoint', 'branch-fail', extractStderr(err));
  }

  // Step 5 — role-state declare building.
  stepEmit('role-state', 'started');
  try {
    const roleStatePath = path.join(repoRoot, 'platform', 'scripts', 'role-state');
    await execFileAsync(roleStatePath, [role, 'building'], { env, timeout: 10_000 });
    stepEmit('role-state', 'completed');
  } catch {
    // Non-fatal — board state is already updated; role-state is a session-attention hint.
    stepEmit('role-state', 'completed', { warning: 'role-state declare failed (non-fatal)' });
  }

  // Step 6 — spine event card.pulled.
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
// uncommitted work. Reuses chorus-werk close for the detach + branch
// teardown + werk.detached spine event.
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

  const stepEmit = (step: string, status: 'started' | 'completed', detail?: Record<string, unknown>) =>
    emit(`chorus_unpull_card.${step}.${status}`, { role, card_id: cardId, ...(detail ?? {}) });

  emit('chorus_unpull_card.invoked', { role, card_id: cardId, repo_root: repoRoot });

  const path = require('path') as typeof import('path');
  const parentNodeBinDir = path.dirname(process.execPath);
  const cargoBinDir = `${process.env.HOME ?? ''}/.cargo/bin`;
  const env = {
    ...process.env,
    DEPLOY_ROLE: role,
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

  // Step 2 — werk pre-flight. Refuse on missing werk path (#2760), exec
  // failures (werk-corrupt), or uncommitted changes (don't lose work).
  stepEmit('werk-preflight', 'started');
  if (!fsExists(repoRoot)) {
    refuse('werk-preflight', 'werk-not-initialized', `werk path does not exist: ${repoRoot} — run \`chorus-werk init ${role}\` first`);
  }
  if (!fsExists(path.join(repoRoot, '.git'))) {
    refuse('werk-preflight', 'werk-not-initialized', `werk path exists but is not a git worktree: ${repoRoot} — run \`chorus-werk init ${role}\` first`);
  }
  let dirty = '';
  try {
    const r = await execFileAsync('git', ['status', '--porcelain'], { env, cwd: repoRoot, timeout: 5_000 });
    dirty = r.stdout;
  } catch (err) {
    refuse('werk-preflight', 'werk-corrupt', `git status failed at ${repoRoot}: ${extractStderr(err)}`);
  }
  if (dirty.trim().length > 0) {
    refuse('werk-preflight', 'werk-dirty', `werk has uncommitted changes:\n${dirty.trim()}`);
  }
  stepEmit('werk-preflight', 'completed');

  // Step 3 — cards move <id> Next.
  stepEmit('cards-move', 'started');
  try {
    await execFileAsync(cardsPath, ['move', String(cardId), 'Next'], { env, timeout: 15_000 });
    stepEmit('cards-move', 'completed');
  } catch (err) {
    const stderr = extractStderr(err);
    if (!/already.*Next|already in Next/i.test(stderr)) {
      refuse('cards-move', 'move-fail', stderr);
    }
    stepEmit('cards-move', 'completed', { idempotent: true });
  }

  // Step 4 — chorus-werk close <role> <card_id>. Reuses #2740/#2757
  // implementation: detach werk to origin/main, delete local branch,
  // best-effort delete remote, emit werk.detached spine event. Pass
  // --no-done-check because the card just went WIP→Next, not Done.
  stepEmit('werk-close', 'started');
  let branchClosed = false;
  try {
    const chorusWerkPath = path.join(repoRoot, 'platform', 'scripts', 'chorus-werk');
    await execFileAsync(chorusWerkPath, ['close', '--no-done-check', role, String(cardId)], { env, timeout: 30_000 });
    branchClosed = true;
    stepEmit('werk-close', 'completed', { branch_closed: true });
  } catch (err) {
    const stderr = extractStderr(err);
    if (/already closed|no local ref/i.test(stderr)) {
      // Idempotent: branch was already torn down.
      branchClosed = true;
      stepEmit('werk-close', 'completed', { idempotent: true });
    } else {
      refuse('werk-close', 'branch-close-fail', stderr);
    }
  }

  // Step 5 — role-state idle (best-effort; non-fatal).
  stepEmit('role-state', 'started');
  try {
    const roleStatePath = path.join(repoRoot, 'platform', 'scripts', 'role-state');
    await execFileAsync(roleStatePath, [role, 'idle'], { env, timeout: 10_000 });
    stepEmit('role-state', 'completed');
  } catch {
    stepEmit('role-state', 'completed', { warning: 'role-state idle failed (non-fatal)' });
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
  const shimPath = deps.shimPath ?? resolveShimPath();
  const cardsPath = deps.cardsPath ?? resolveCardsPath();
  const fetchImpl: FetchImpl = deps.fetchImpl ?? (globalThis.fetch as unknown as FetchImpl);
  const apiBase = deps.apiBase ?? 'http://localhost:3340';
  const boardReader: BoardReader = deps.boardReader ?? defaultBoardReader(fetchImpl, apiBase);
  const emitSpineEvent: SpineEmitter = deps.emitSpineEvent ?? defaultSpineEmitter();
  const gitQueuePath = deps.gitQueuePath ?? resolveGitQueuePath();
  // #2750 — werk-aware cwd resolver. Default reads role's settings.json env
  // for CHORUS_WERK_ENABLE; if "1", routes git-queue.sh to /chorus-werk/<role>/.
  // Otherwise returns canonical (#2662 cwd=repo-root contract preserved).
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
        return executePullCard(parsed.data, emitSpineEvent, execFileAsync, cardsPath, resolveWorkingTree, fsExists);
      }
      case 'chorus_unpull_card': {
        const parsed = UnpullCardInput.safeParse(req.params.arguments);
        if (!parsed.success) {
          throw new Error(`Invalid arguments: ${parsed.error.issues.map((i) => i.message).join(', ')}`);
        }
        return executeUnpullCard(parsed.data, emitSpineEvent, execFileAsync, cardsPath, resolveWorkingTree, fsExists);
      }
      default:
        throw new Error(`Unknown tool: ${req.params.name}`);
    }
  });

  return server;
}
