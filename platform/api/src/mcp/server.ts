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
    },
    required: ['role', 'paths', 'message'],
    additionalProperties: false,
  },
} as const;

// #2682 — classify a git-queue.sh non-zero-exit into a typed refusal reason.
// Order-sensitive: check most specific patterns first, fall through to hook-fail.
//   branch-check signature       → branch-mismatch
//   `could not open directory`   → path-not-found (#2662 dogfood receipt)
//   `did not match any files`    → path-not-found
//   anything else                → hook-fail (pre-commit + downstream)
// push-phase failures route through push-conflict; this fn covers commit-phase only.
function classifyCommitFailure(stderr: string): 'branch-mismatch' | 'path-not-found' | 'hook-fail' {
  const lower = stderr.toLowerCase();
  if (lower.includes('branch-check') || /head\s+is\s+\S+\/.+,\s*expected/.test(lower)) {
    return 'branch-mismatch';
  }
  if (lower.includes('could not open directory') || lower.includes('did not match any files')) {
    return 'path-not-found';
  }
  return 'hook-fail';
}

interface CommitArgs {
  role: 'kade' | 'wren' | 'silas';
  paths: string[];
  message: string;
}

async function executeCommit(
  args: CommitArgs,
  boardReader: BoardReader,
  emit: SpineEmitter,
  execFileAsync: ExecFileAsync,
  gitQueuePath: string,
): Promise<{ content: Array<{ type: 'text'; text: string }> }> {
  const { role, paths, message } = args;

  // Step 1 — derive card from board (same path as chorus_commit_status).
  const board = await boardReader(role);
  if (!board.ok) {
    emit(CHORUS_COMMIT_REFUSED, { role, reason: board.reason, detail: board.detail });
    throw new Error(`chorus_commit refused: board-unreachable${board.detail ? ` (${board.detail})` : ''}`);
  }
  if (board.cards.length === 0) {
    emit(CHORUS_COMMIT_REFUSED, { role, reason: 'no-wip-card' });
    throw new Error(`chorus_commit refused: no-wip-card — role ${role} has no card in WIP`);
  }
  if (board.cards.length > 1) {
    const ids = board.cards.map((c) => c.id).join(',');
    emit(CHORUS_COMMIT_REFUSED, { role, reason: 'multi-wip', card_ids: ids });
    throw new Error(`chorus_commit refused: multi-wip — role ${role} has ${board.cards.length} cards in WIP (${ids})`);
  }

  const card = board.cards[0];
  const branch = `${role}/${card.id}`;

  // #2662 — chorus-api's launchctl PATH puts /opt/homebrew/bin first, which
  // is Node 23. The chorus-api process itself runs the team's nvm Node 20
  // (absolute path in launch plist), but subprocess PATH-resolution of
  // `node`/`npx`/`npm` (used by pre-commit's `npx jest`) picks up the
  // Homebrew binary, breaking native modules compiled for Node 20.
  // Prepend the parent node's bin dir so the subprocess chain stays on the
  // same Node version as chorus-api itself.
  const path = require('path') as typeof import('path');
  const parentNodeBinDir = path.dirname(process.execPath);
  const env = {
    ...process.env,
    DEPLOY_ROLE: role,
    PATH: `${parentNodeBinDir}:${process.env.PATH ?? ''}`,
  } as NodeJS.ProcessEnv;

  // #2662 — git-queue.sh stages paths via `git add <path>` which resolves
  // them relative to cwd. chorus-api runs from platform/api, so without
  // an explicit cwd, paths like "skills/acp/SKILL.md" became
  // "platform/api/skills/acp/SKILL.md" (404). Derive repo root from the
  // gitQueuePath = "<repo>/platform/scripts/git-queue.sh".
  const repoRoot = gitQueuePath.replace(/\/platform\/scripts\/git-queue\.sh$/, '');

  // Step 2 — commit via git-queue.sh. `<paths> -- -m <message>` is the contract.
  let commitStdout: string;
  try {
    const commitArgs = ['commit', ...paths, '--', '-m', message];
    const { stdout } = await execFileAsync(gitQueuePath, commitArgs, { env, timeout: 30_000, cwd: repoRoot });
    commitStdout = stdout;
  } catch (err) {
    const stderr = (err as { stderr?: string }).stderr ?? (err instanceof Error ? err.message : String(err));
    const reason = classifyCommitFailure(stderr);
    emit(CHORUS_COMMIT_REFUSED, { role, card_id: card.id, reason, detail: stderr.slice(0, 500) });
    throw new Error(`chorus_commit refused: ${reason} — ${stderr.split('\n')[0]}`);
  }

  // Extract SHA from `[branch sha] message` line (git's standard commit output).
  const shaMatch = commitStdout.match(/\[\S+\s+([a-f0-9]+)\]/);
  const sha = shaMatch ? shaMatch[1] : 'unknown';

  // Step 3 — push via git-queue.sh (rebase-on-conflict, race-safe under lock).
  try {
    await execFileAsync(gitQueuePath, ['push'], { env, timeout: 60_000, cwd: repoRoot });
  } catch (err) {
    const stderr = (err as { stderr?: string }).stderr ?? (err instanceof Error ? err.message : String(err));
    emit(CHORUS_COMMIT_REFUSED, { role, card_id: card.id, reason: 'push-conflict', detail: stderr.slice(0, 500) });
    throw new Error(`chorus_commit refused: push-conflict — ${stderr.split('\n')[0]}`);
  }

  // Step 4 — success. Emit invoked and return structured payload.
  emit(CHORUS_COMMIT_INVOKED, { role, card_id: card.id, paths_count: paths.length, sha });
  return {
    content: [
      {
        type: 'text',
        text: JSON.stringify({ role, card_id: card.id, branch, sha }, null, 2),
      },
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
        return executeCommit(parsed.data, boardReader, emitSpineEvent, execFileAsync, gitQueuePath);
      }
      default:
        throw new Error(`Unknown tool: ${req.params.name}`);
    }
  });

  return server;
}
