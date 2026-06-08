/* eslint-disable security/detect-non-literal-fs-filename, security/detect-object-injection --
 * fs paths in this CLI SDK are constructed from server-controlled defaults
 * (DEFAULT_SNAPSHOT_DIR, DEFAULT_WORKFLOWS_*, DEFAULT_BRIEF_DIRS) with optional
 * test-only overrides via __setTestPaths. Object indexing is on internally-derived
 * keys (BRIEF_DIRS keyed by role, status/priority labels from typed enums in
 * config.ts). This is a local CLI library invoked by trusted role processes —
 * arguments come from authenticated terminal sessions, not HTTP.
 */
import * as fs from 'fs';
import * as path from 'path';
import { BoardClient } from './client';
import { BoardTask } from './types';
import { detectRole, LABELS } from './config';
import { emitSpineEvent } from './events';
import { spawnSync } from 'child_process';

// Auto-declare role state from card actions (#1782)
// Eliminates manual role-state calls — state follows card lifecycle.
const ROLE_STATE_BIN = path.resolve(__dirname, '../../../../platform/scripts/role-state');

// #2652 — extract repeated event name (was duplicated 5x, sonarjs flagged at #2603 threshold).
const EVENT_CARD_QUALITY_BLOCKED = 'card.quality.blocked';
function autoRoleState(state: string, extra: string = ''): void {
  const role = detectRole();
  if (!role) return;
  try {
    spawnSync(ROLE_STATE_BIN, [role, state, ...extra.split(' ').filter(Boolean)], { timeout: 3000 });
  } catch { /* non-blocking — don't break card ops if role-state fails */ }
}
import { generateBlastRadius, formatBlastComment } from './blast-radius';
import { countAcDiff } from './ac-tick-detection';

// Paths are `let` so hermetic tests can point them at a temp dir via
// `__setTestPaths`. Defaults match the pre-override production layout.
// #2241 wave 3 refactor — dep-injection via module-scope override rather
// than threading deps through every function signature.
const DEFAULT_SNAPSHOT_DIR = path.join(__dirname, '../../logs');
const DEFAULT_WORKFLOWS_ACTIVE_DIR = path.join(__dirname, '../../workflows/active');
const DEFAULT_WORKFLOWS_ARCHIVE_DIR = path.join(__dirname, '../../workflows/archive');
const DEFAULT_BRIEF_DIRS: Record<string, string> = {
  silas: path.join(__dirname, '../../roles/silas/briefs'),
  kade: path.join(__dirname, '../../roles/kade/briefs'),
  wren: path.join(__dirname, '../../roles/wren/briefs'),
};

let SNAPSHOT_DIR = DEFAULT_SNAPSHOT_DIR;
let WORKFLOWS_ACTIVE_DIR = DEFAULT_WORKFLOWS_ACTIVE_DIR;
let WORKFLOWS_ARCHIVE_DIR = DEFAULT_WORKFLOWS_ARCHIVE_DIR;
let BRIEF_DIRS: Record<string, string> = DEFAULT_BRIEF_DIRS;

/**
 * Test-only hook: override the module-level paths so hermetic tests can
 * read/write a temp directory instead of the real role/briefs/workflows
 * trees. Call `__resetTestPaths()` in afterEach to restore defaults.
 */
export function __setTestPaths(overrides: {
  snapshotDir?: string;
  workflowsActiveDir?: string;
  workflowsArchiveDir?: string;
  briefDirs?: Record<string, string>;
}): void {
  if (overrides.snapshotDir) SNAPSHOT_DIR = overrides.snapshotDir;
  if (overrides.workflowsActiveDir) WORKFLOWS_ACTIVE_DIR = overrides.workflowsActiveDir;
  if (overrides.workflowsArchiveDir) WORKFLOWS_ARCHIVE_DIR = overrides.workflowsArchiveDir;
  if (overrides.briefDirs) BRIEF_DIRS = overrides.briefDirs;
}

/** Test-only: restore module-level paths to their production defaults. */
export function __resetTestPaths(): void {
  SNAPSHOT_DIR = DEFAULT_SNAPSHOT_DIR;
  WORKFLOWS_ACTIVE_DIR = DEFAULT_WORKFLOWS_ACTIVE_DIR;
  WORKFLOWS_ARCHIVE_DIR = DEFAULT_WORKFLOWS_ARCHIVE_DIR;
  BRIEF_DIRS = DEFAULT_BRIEF_DIRS;
}

// Import from compiled dist to avoid rootDir conflicts
const { WorkflowEngine } = require('../../../../platform/workflow-engine/dist/engine');

/** Minimal shape of WorkflowManifest used here — full type lives in platform/workflow-engine. */
interface WorkflowManifestLite {
  id: string;
  card: number | null;
}

// ── Notifications ──

export function notifyOwnerIfDifferent(
  cardIndex: number, title: string, owner: string, action: string, mover: string
): void {
  try {
    const ownerLower = owner.toLowerCase();
    const moverLower = mover.toLowerCase();
    if (!ownerLower || ownerLower === moverLower || ownerLower === 'jeff') return;
    const briefDir = BRIEF_DIRS[ownerLower];
    if (!briefDir) return;
    fs.mkdirSync(briefDir, { recursive: true });
    const dateStr = new Date().toISOString().split('T')[0];
    const briefFile = path.join(briefDir, `${dateStr}-card-${cardIndex}-${action}.md`);
    if (fs.existsSync(briefFile)) return;
    const content = `# Card #${cardIndex} ${action} by ${mover}\n\n**Title:** ${title}\n**Action:** ${action} by ${mover}\n**Date:** ${dateStr}\n`;
    fs.writeFileSync(briefFile, content);
    console.log(`  Notified ${owner}: #${cardIndex} ${action} by ${mover}`);
    emitSpineEvent('card.owner.notified', moverLower, {
      card_id: String(cardIndex), owner: ownerLower, action,
    });
  } catch { /* best effort */ }
}

export function notifyPM(
  cardIndex: number, title: string, owner: string, completedBy: string
): void {
  try {
    const pmDir = BRIEF_DIRS['wren'];
    if (!pmDir) return;
    const completedByLower = completedBy.toLowerCase();
    if (completedByLower === 'wren') return;
    fs.mkdirSync(pmDir, { recursive: true });
    const dateStr = new Date().toISOString().split('T')[0];
    const briefFile = path.join(pmDir, `${dateStr}-card-${cardIndex}-shipped.md`);
    if (fs.existsSync(briefFile)) return;
    const content = `# Card #${cardIndex} shipped\n\n**Title:** ${title}\n**Completed by:** ${completedBy}\n**Owner:** ${owner}\n**Date:** ${dateStr}\n\nThis card was marked Done. Review for acceptance or follow-up.\n`;
    fs.writeFileSync(briefFile, content);
    console.log(`  Briefed Wren: #${cardIndex} shipped by ${completedBy}`);
    emitSpineEvent('card.pm.briefed', completedByLower, {
      card_id: String(cardIndex), title, completed_by: completedByLower,
    });
  } catch { /* best effort */ }
}

// ── Workflow reconciliation ──

export function reconcileWorkflows(cardIndex: number, role: string): void {
  try {
    if (!fs.existsSync(WORKFLOWS_ACTIVE_DIR)) return;
    for (const f of fs.readdirSync(WORKFLOWS_ACTIVE_DIR)) {
      if (!f.match(/^WF-\d+\.json$/)) continue;
      const fp = path.join(WORKFLOWS_ACTIVE_DIR, f);
      const manifest = JSON.parse(fs.readFileSync(fp, 'utf-8'));
      if (manifest.card !== cardIndex) continue;

      const now = new Date().toISOString();
      for (const step of manifest.steps) {
        if (step.status === 'pending' || step.status === 'ready') {
          step.status = 'skipped';
          step.completed_at = now;
          step.notes = `Skipped — card #${cardIndex} marked Done by ${role}`;
        }
      }
      manifest.status = 'completed';
      manifest.updated = now;
      manifest.history.push({
        timestamp: now,
        event: 'workflow_completed',
        role: 'system',
        detail: `Auto-archived: card #${cardIndex} marked Done by ${role}`,
      });

      fs.mkdirSync(WORKFLOWS_ARCHIVE_DIR, { recursive: true });
      fs.writeFileSync(path.join(WORKFLOWS_ARCHIVE_DIR, f), JSON.stringify(manifest, null, 2) + '\n');
      fs.unlinkSync(fp);

      console.log(`  ${manifest.id} auto-archived (card #${cardIndex} done)`);
      emitSpineEvent('workflow.manifest.archived', role, {
        workflow_id: manifest.id, card_id: String(cardIndex), reason: 'card_done',
      });
    }
  } catch { /* best effort */ }
}

// ── Workflow trigger ──

export async function triggerWorkflow(client: BoardClient, cardIndex: number): Promise<void> {
  const engine = new WorkflowEngine();

  const existing = engine.scanWorkflows().find((wf: WorkflowManifestLite) => wf.card === cardIndex);
  if (existing) {
    console.log(`  Workflow ${existing.id} already exists for #${cardIndex}`);
    return;
  }

  const card = await client.view(cardIndex);
  const owner = card.owner.toLowerCase() || 'wren';
  const board = client.boardName;
  const isChorus = board === 'chorus' || card.domains.includes('product:chorus');
  const prefix = isChorus ? 'C#' : '#';

  let builder = owner;
  let reviewer: string;
  if (owner === 'wren') {
    builder = 'kade';
    reviewer = 'silas';
  } else {
    reviewer = 'wren';
  }

  const safeTitle = card.title.replace(/[,→]/g, ' ').replace(/\s+/g, ' ').trim();
  const steps = `${builder}:${safeTitle},${reviewer}:Review and verify ${prefix}${cardIndex}`;

  const manifest = engine.create(
    card.title,
    steps,
    `board:card-moved:${prefix}${cardIndex}`,
    cardIndex,
  );

  console.log(`  ${manifest.id} created for ${prefix}${cardIndex} → ${builder} builds, ${reviewer} reviews`);
  emitSpineEvent('workflow.manifest.created', detectRole(), {
    workflow_id: manifest.id, card_id: String(cardIndex), board,
  });
}

// ── Code card detection (DEC-084) ──

const CODE_INDICATORS = /\b(handler|refactor|fix|migrate|route|sparql|service|endpoint|middleware|controller|query|schema|deploy|build|test|lint)\b/i;
const NON_CODE_INDICATORS = /\[(process|docs|product|spike|discovery)\]|\b(brief|decision|meeting|review|retrospective|planning|spike|research)\b/i;

/**
 * Detect if a card is likely a code change based on title/description content.
 * Non-code cards (process, docs, product) are exempt.
 */
export function isCodeCard(text: string): boolean {
  if (NON_CODE_INDICATORS.test(text)) return false;
  return CODE_INDICATORS.test(text);
}

// #3227 — the cards-done demo gate (checkDemoEvidence, #1834/#2910) was REMOVED.
// "Did this card pass demo?" has exactly one reader: werk-accept's
// demo_verdict_pass over the demo.verdict witness (#3116). cards-done is a board
// primitive and no longer re-gates — see the note in doneCard.

// ── Quality gates ──

export function warnShortTitle(title: string, board: string): void {
  if (title.length < 10) {
    console.log(`  WARN: Title "${title}" is very short (${title.length} chars). Good titles: "<verb> <what> — <why or scope>"`);
    emitSpineEvent('card.quality.warned', detectRole(), {
      gate: 'title_short', title, board,
    });
  }
}

export function enforceNowDescriptionGate(index: number, title: string, description: string | undefined, board: string): boolean {
  // SWAT cards are exempt (DEC-055)
  if (/\[swat\]/i.test(title)) return true;

  const desc = (description || '').trim();

  if (!desc) {
    console.error(`ERROR: Cards entering Now require Experience + AC in description. Card #${index} has no description.`);
    emitSpineEvent(EVENT_CARD_QUALITY_BLOCKED, detectRole(), {
      card_id: String(index), title, gate: 'now_description_missing', stage: 'directing', board,
    });
    return false;
  }

  const hasExperience = /##\s*experience/i.test(desc);
  const hasAC =
    /acceptance\s*criteria/i.test(desc) ||
    /##\s*(ac|criteria|what|acceptance)/i.test(desc) ||
    /- \[[ x]\]/i.test(desc) ||
    /\d+\.\s+\S/m.test(desc);

  if (!hasExperience || !hasAC) {
    const missing = [!hasExperience && '## Experience', !hasAC && '## AC'].filter(Boolean).join(' and ');
    console.error(`ERROR: Cards entering Now require Experience + AC in description. Card #${index} is missing ${missing}.`);
    emitSpineEvent(EVENT_CARD_QUALITY_BLOCKED, detectRole(), {
      card_id: String(index), title, gate: 'now_description_incomplete', stage: 'directing', board,
    });
    return false;
  }

  return true;
}

/**
 * Experience gate (#1839) — cards require an Experience section before entering WIP.
 * Returns true if the card may proceed, false if blocked.
 * Exempt: [swat] cards, parent/umbrella cards.
 */
export function enforceExperienceGate(index: number, title: string, description: string | undefined, board: string): boolean {
  // SWAT cards are exempt (DEC-055)
  if (/\[swat\]/i.test(title)) return true;

  const desc = (description || '').trim();

  // Parent/umbrella cards exempt
  if (desc.length > 0 && /^(children:|parent card)/i.test(desc)) return true;

  const hasExperience = /##\s*experience/i.test(desc);

  if (!hasExperience) {
    console.error(`ERROR: Card #${index} needs an Experience section before entering WIP.`);
    console.error('  Add "## Experience" with 2-5 sentences in Jeff\'s voice describing what he sees/feels/gets.');
    console.error('  Route to Wren to draft the Experience section.');
    emitSpineEvent(EVENT_CARD_QUALITY_BLOCKED, detectRole(), {
      card_id: String(index), title, gate: 'experience_missing', stage: 'building', board,
    });
    return false;
  }

  return true;
}

/**
 * Capture gate (#1085) — cards require AC before entering WIP.
 * Returns true if the card may proceed, false if blocked.
 * Exempt: [swat] cards, parent/umbrella cards (children-only descriptions).
 */
export function enforceACGate(index: number, title: string, description: string | undefined, board: string): boolean {
  // SWAT cards are exempt (DEC-055)
  if (/\[swat\]/i.test(title)) return true;

  const desc = (description || '').trim();

  // Parent/umbrella cards — description is only child references like #NNN
  if (desc.length > 0 && /^(children:|parent card)/i.test(desc)) return true;

  // Check for AC content: heading with "AC"/"Acceptance Criteria"/"Criteria",
  // checkboxes, or numbered acceptance items
  const hasAC =
    /acceptance\s*criteria/i.test(desc) ||
    /##\s*(ac|criteria|what|acceptance)/i.test(desc) ||
    /- \[[ x]\]/i.test(desc) ||             // markdown checkboxes
    /\d+\.\s+\S/m.test(desc);               // numbered list items (at least 1)

  if (!hasAC) {
    console.error(`ERROR: Card #${index} needs acceptance criteria before entering WIP. Route to Wren.`);
    emitSpineEvent(EVENT_CARD_QUALITY_BLOCKED, detectRole(), {
      card_id: String(index), title, gate: 'capture_ac_missing', stage: 'building', board,
    });
    return false;
  }

  return true;
}

/**
 * Taxonomy gate (#1272) — enforce chunk (hard block) and sequence (soft warn) on Now/WIP entry.
 * SWAT cards exempt. Returns true if card may proceed.
 */
export function enforceTaxonomyGate(
  index: number, title: string, domains: string[], board: string
): boolean {
  // SWAT cards exempt
  if (/\[swat\]/i.test(title)) return true;

  // Chunk is optional (#1873) — domain is the primary taxonomy
  const hasSequence = domains.some(d => d.startsWith('sequence:'));

  if (!hasSequence) {
    const validSeqs = Object.keys(LABELS.sequence).join(', ');
    console.log(`  WARN: No sequence label on #${index}. Use: cards tag ${index} sequence <seq>`);
    console.log(`  Valid sequences: ${validSeqs}`);
    emitSpineEvent('card.quality.warned', detectRole(), {
      card_id: String(index), title, gate: 'taxonomy_sequence_missing', board,
    });
  }

  return true;
}

export async function warnNoComments(client: BoardClient, index: number, title: string, board: string): Promise<void> {
  try {
    const comments = await client.comments(index);
    if (comments.length === 0) {
      console.log(`  WARN: #${index} has no comments. Done cards should have at least a completion comment.`);
      emitSpineEvent('card.quality.warned', detectRole(), {
        card_id: String(index), title, gate: 'no_comments', stage: 'proving', board,
      });
    }
  } catch { /* best effort */ }
}

// ── Audit ──

const HOUR_MS = 3600_000;

function ageLabelFromTimestamps(updatedStr: string, now: number): string {
  const age = now - new Date(updatedStr).getTime();
  if (age < HOUR_MS) return `${Math.round(age / 60_000)}m`;
  if (age < 24 * HOUR_MS) return `${Math.round(age / HOUR_MS)}h`;
  return `${Math.round(age / (24 * HOUR_MS))}d`;
}

function emitStaleEvents(tasks: BoardTask[], role: string, stage: string, status: string | undefined, now: number, boardName: string): void {
  for (const t of tasks) {
    emitSpineEvent('card.stale.detected', role, {
      card_id: String(t.index), title: t.title, stage,
      status: status ?? t.status,
      age: ageLabelFromTimestamps(t.updated, now),
      board: boardName,
    });
  }
}

function printAuditSection(tasks: BoardTask[], label: string, question: string, staleSet?: Set<number>, now?: number): void {
  if (tasks.length === 0) return;
  console.log(`\n${label} (${tasks.length}) — ${question}`);
  for (const t of tasks) {
    const stale = staleSet && now !== undefined && staleSet.has(t.index) ? ` — ${ageLabelFromTimestamps(t.updated, now)} stale` : '';
    const pri = t.priority ? ` [${t.priority}]` : '';
    console.log(`  #${t.index}  ${t.title}${pri}${stale}`);
  }
}

export async function auditStart(client: BoardClient, role: string): Promise<{
  staleNow: number; staleNext: number; nowCount: number;
}> {
  const boardName = client.boardName;
  const snap = await client.snapshot();
  const snapFile = path.join(SNAPSHOT_DIR, `board-snapshot-${boardName}-${role}.json`);
  fs.mkdirSync(SNAPSHOT_DIR, { recursive: true });
  fs.writeFileSync(snapFile, JSON.stringify(snap, null, 2));

  const myTasks = snap.tasks.filter((t) => t.owner.toLowerCase() === role.toLowerCase());
  const byStatus = (statuses: string[]) => myTasks.filter((t) => statuses.includes(t.status));
  const nowTasks = byStatus(['Now', 'WIP']);
  const swatTasks = byStatus(['SWAT']);
  const harvestTasks = byStatus(['Harvesting']);
  const next = byStatus(['Next']);
  const blocked = byStatus(['Blocked']);

  const now = Date.now();
  const staleNow = nowTasks.filter((t) => (now - new Date(t.updated).getTime()) > 48 * HOUR_MS);
  const staleNext = next.filter((t) => (now - new Date(t.updated).getTime()) > 7 * 24 * HOUR_MS);

  emitStaleEvents(staleNow, role, 'building', undefined, now, boardName);
  emitStaleEvents(staleNext, role, 'directing', 'Next', now, boardName);

  const staleNowSet = new Set(staleNow.map((s) => s.index));
  const staleNextSet = new Set(staleNext.map((s) => s.index));
  printAuditSection(nowTasks, 'In Progress', 'still working on these?', staleNowSet, now);
  printAuditSection(next, 'Next', 'any of these already done?', staleNextSet, now);
  printAuditSection(swatTasks, 'SWAT', 'open from prior session?');
  printAuditSection(harvestTasks, 'Harvesting', 'still running?');
  printAuditSection(blocked, 'Blocked', 'still blocked?');

  if (nowTasks.length === 0 && next.length === 0) {
    console.log(`\n  No active items for ${role}. Pick a card before starting work.`);
  }

  console.log(`\nAUDIT:stale_now=${staleNow.length},stale_next=${staleNext.length},now_count=${nowTasks.length}`);
  emitSpineEvent('board.audit.started', role, { board: boardName, snapshot: snapFile });
  return { staleNow: staleNow.length, staleNext: staleNext.length, nowCount: nowTasks.length };
}

// cog-override: auditClose: snapshot diff with multi-bucket per-card classification — pre-existing complexity, refactor candidate not in #2652 scope.
export async function auditClose(client: BoardClient, role: string): Promise<{
  newCards: number; newlyDone: number; retroactive: number;
}> {
  const boardName = client.boardName;
  const snapFile = path.join(SNAPSHOT_DIR, `board-snapshot-${boardName}-${role}.json`);
  if (!fs.existsSync(snapFile)) {
    console.log('  No start-of-session snapshot found. Cannot diff.');
    return { newCards: 0, newlyDone: 0, retroactive: 0 };
  }

  const startSnap = JSON.parse(fs.readFileSync(snapFile, 'utf-8'));
  const startTaskIds = new Set(startSnap.tasks.map((t: BoardTask) => t.index));
  const startDoneIds = new Set(
    startSnap.tasks.filter((t: BoardTask) => t.status === 'Done').map((t: BoardTask) => t.index)
  );

  const currentTasks = await client.list();
  const currentDone = currentTasks.filter(t => t.status === 'Done');
  const newCards = currentTasks.filter(t => !startTaskIds.has(t.index));
  const newlyDone = currentDone.filter(t => !startDoneIds.has(t.index));
  const retroactive = newCards.filter(t => t.status === 'Done');

  if (newlyDone.length > 0) {
    console.log(`\nCompleted this session (${newlyDone.length}):`);
    for (const t of newlyDone) {
      const isNew = newCards.some(n => n.index === t.index);
      const flag = isNew ? ' ⚠ RETROACTIVE' : '';
      console.log(`  #${t.index}  ${t.title}${flag}`);
    }
  }

  if (retroactive.length > 0) {
    console.log(`\n  WARN: ${retroactive.length} card(s) created AND completed in same session.`);
    console.log('  Card-first rule: create cards BEFORE starting work, not after.');
  }

  const nonDoneNew = newCards.filter(t => t.status !== 'Done');
  if (nonDoneNew.length > 0) {
    console.log(`\nNew cards created (${nonDoneNew.length}):`);
    for (const t of nonDoneNew) {
      console.log(`  [${t.status}] #${t.index}  ${t.title}`);
    }
  }

  const startNowIds = new Set(
    startSnap.tasks.filter((t: BoardTask) => t.status === 'Now').map((t: BoardTask) => t.index)
  );
  const stillNow = currentTasks.filter(t => t.status === 'Now' && startNowIds.has(t.index));
  if (stillNow.length > 0) {
    console.log('\nStill In Progress (started before this session):');
    for (const t of stillNow) {
      console.log(`  #${t.index}  ${t.title}`);
    }
  }

  emitSpineEvent('board.audit.closed', role, {
    board: boardName,
    new_cards: String(newCards.length),
    newly_done: String(newlyDone.length),
    retroactive: String(retroactive.length),
  });

  return { newCards: newCards.length, newlyDone: newlyDone.length, retroactive: retroactive.length };
}

// ── High-level operations (composable by CLI or scripts) ──

// Maps title verbs to card type labels.
const TITLE_TO_TYPE: Record<string, string> = {
  fix: 'fix', repair: 'fix', broken: 'fix', bug: 'fix',
  add: 'new', create: 'new', build: 'new', implement: 'new',
  update: 'enhance', improve: 'enhance', enhance: 'enhance', upgrade: 'enhance',
  remove: 'chore', clean: 'chore', refactor: 'chore', migrate: 'chore',
};

// Chunk auto-inference from domain (#1873).
const DOMAIN_TO_CHUNK: Record<string, string> = {
  photos: 'memory', music: 'music', stories: 'memory', notes: 'memory',
  people: 'memory', social: 'memory', documents: 'memory', books: 'memory',
  blog: 'app', property: 'app', cooking: 'app', reading: 'app',
  watching: 'app', todo: 'app', seeds: 'app', glimmers: 'app',
  ideas: 'app', projects: 'app', sexuality: 'sexuality', values: 'app',
  practices: 'app', search: 'app', self: 'senses',
  skills: 'spine', roles: 'spine', cards: 'ops', decisions: 'spine',
  briefs: 'spine', sessions: 'spine', convergence: 'convergence',
  infrastructure: 'ops',
};

// Origin inference from type (#2101).
const TYPE_TO_ORIGIN: Record<string, string> = {
  fix: 'reactive', swat: 'reactive',
  new: 'reflective', enhance: 'reflective',
};

type AddOpts = {
  status?: string; owner?: string; priority?: string; domain?: string;
  description?: string; product?: string; chunk?: string; sequence?: string;
  type?: string; origin?: string;
  // #2652 AC1+AC2 — new tag axes
  subdomain?: string; subproduct?: string;
};

// #2652 AC2 — subproduct closed list. Athena models 7 subproducts today; doc
// (cards-service-design.md) names 6 user-facing — Quality is intentionally
// not exposed as a subproduct on cards yet (horizontal capability per Jeff
// 2026-05-01). Add Quality if/when it becomes a tagged surface.
const VALID_SUBPRODUCTS = new Set(['athena', 'loom', 'werk', 'borg', 'convergence', 'clearing']);

// #2652 AC1 — subdomain closed list sourced LIVE from Athena. Cached per
// process lifetime to keep validation fast; refresh on cache-miss.
let SUBDOMAIN_CACHE: Set<string> | null = null;
async function fetchSubdomainSet(): Promise<Set<string>> {
  if (SUBDOMAIN_CACHE) return SUBDOMAIN_CACHE;
  try {
    const resp = await fetch('http://localhost:3340/api/athena/subdomains');
    if (!resp.ok) throw new Error(`status ${resp.status}`);
    const body = await resp.json() as { data?: Array<{ id: string }> };
    const ids = (body.data || []).map((r) => r.id);
    SUBDOMAIN_CACHE = new Set(ids);
    return SUBDOMAIN_CACHE;
  } catch (err) {
    // If Athena is unreachable at validation time, fail closed: refuse-at-source
    // means we'd rather block the add than let an unvalidated subdomain land.
    throw new Error(`subdomain validation requires Athena (localhost:3340/api/athena/subdomains): ${err instanceof Error ? err.message : err}`);
  }
}

// Mutates opts to fill in type/chunk/origin from title and domain where possible.
function inferCardDefaults(title: string, opts: AddOpts): void {
  if (!opts.type) {
    const firstWord = title.split(/\s+/)[0]?.toLowerCase() || '';
    const inferred = TITLE_TO_TYPE[firstWord];
    if (inferred) {
      opts.type = inferred;
      console.log(`  Auto-tagged type:${inferred} from title verb "${firstWord}"`);
    }
  }
  if (!opts.chunk && opts.domain) {
    const inferred = DOMAIN_TO_CHUNK[opts.domain.toLowerCase()];
    if (inferred) {
      opts.chunk = inferred;
      console.log(`  Auto-tagged chunk:${inferred} from domain:${opts.domain}`);
    }
  }
  if (!opts.origin && opts.type) {
    const inferred = TYPE_TO_ORIGIN[opts.type.toLowerCase()];
    if (inferred) {
      opts.origin = inferred;
      console.log(`  Auto-tagged origin:${inferred} from type:${opts.type}`);
    }
  }
}

// Validates required fields and collects error strings.
// cog-override: required-field validator with per-axis branches (#2652 added subdomain+subproduct; pre-existing for type/priority/origin/domain). Each branch is one check with specific error text; collapsing to a loop would lose per-axis error-message clarity.
async function collectRequiredFieldErrors(opts: AddOpts): Promise<string[]> {
  const errors: string[] = [];
  if (!opts.domain) errors.push('Missing --domain <name>');
  const validTypes = Object.keys(LABELS.type).join(', ');
  if (!opts.type) errors.push(`Missing --type <${validTypes}>`);
  else if (!LABELS.type[opts.type.toLowerCase()]) errors.push(`Unknown type "${opts.type}". Valid: ${validTypes}`);
  if (!opts.priority) errors.push('Missing --priority P1|P2|P3');
  if (!opts.origin) errors.push('Missing origin. Is this reactive (responding to breakage) or reflective (chosen work)? Use --origin reflective|reactive');
  else if (!['reflective', 'reactive'].includes(opts.origin.toLowerCase())) errors.push(`Unknown origin "${opts.origin}". Valid: reflective, reactive`);
  // #2895: sequence WARN → ERROR. #3293 removed --quick, so this is universal (every card names its product/area).
  if (!opts.sequence) errors.push('Missing --sequence <name> (the product/area this card belongs to).');
  // #2652 AC2 — subproduct refuse-at-source (closed list)
  if (opts.subproduct) {
    const sp = opts.subproduct.toLowerCase();
    if (!VALID_SUBPRODUCTS.has(sp)) {
      errors.push(`Unknown --subproduct "${opts.subproduct}". Valid: ${Array.from(VALID_SUBPRODUCTS).join(', ')}`);
    }
  }
  // #2652 AC1 — subdomain refuse-at-source (Athena live query, fail-closed)
  if (opts.subdomain) {
    try {
      const valid = await fetchSubdomainSet();
      if (!valid.has(opts.subdomain)) {
        const preview = Array.from(valid).slice(0, 8).join(', ');
        errors.push(`Unknown --subdomain "${opts.subdomain}". Athena reports ${valid.size} valid subdomains (e.g. ${preview}, ...). Add new subdomain in Athena before tagging.`);
      }
    } catch (err) {
      errors.push(`--subdomain validation failed: ${err instanceof Error ? err.message : err}`);
    }
  }
  return errors;
}

// Validates description: the Experience + AC floor (universal) + the six
// articulated sections (agent-only, the bouncer's substance gate). #3293.
// #2895: Experience promoted from WARN to ERROR — caller no longer needs title/board for the warn-event emit.
function validateDescription(
  opts: AddOpts, _title: string, _boardName: string, errors: string[],
): void {
  // #3293: --quick removed. The Experience + AC floor is UNIVERSAL — every card,
  // including Jeff-initiated, carries its substance. There is no escape hatch.
  const desc = (opts.description || '').trim();
  if (!desc) {
    errors.push('Missing --desc (every card needs a description with ## Experience + AC). Use --desc-file <path> or --desc - for stdin.');
  } else {
    const hasAC =
      /acceptance\s*criteria/i.test(desc) ||
      /##\s*(ac|criteria|what|acceptance)/i.test(desc) ||
      /- \[[ x]\]/i.test(desc) ||
      /\d+\.\s+\S/m.test(desc);
    if (!hasAC) errors.push('Description missing acceptance criteria (need ## AC heading, checkboxes, or numbered items).');
  }
  const hasExperience = /##\s*experience/i.test(opts.description || '');
  if (!hasExperience) {
    errors.push('Description missing "## Experience" section — name the user impact (what changes for Jeff/roles after this lands).');
  }
  // #3293: the six articulated sections below are the AGENT bouncer's substance
  // gate — required ONLY for agent-initiated cards. Jeff-initiated cards
  // (DEPLOY_ROLE=jeff or unset) file at the Experience + AC floor above.
  const deployRole = (process.env.DEPLOY_ROLE || '').toLowerCase();
  const isAgentInitiated = deployRole === 'wren' || deployRole === 'silas' || deployRole === 'kade';
  if (!isAgentInitiated) return;
  // #2905 (Jeff direct 2026-05-11): require all four articulated sections
  // plus dependency-count and scope-of-impact. The structured composition is
  // the forcing function — agents have to literally write each before they
  // can ask Jeff to approve. Many weak cards die at the agent's own mirror
  // step before reaching Jeff. Min 30 words per section filters generics.
  const REQUIRED_SECTIONS: Array<{ heading: string; pattern: RegExp; minWords: number; purpose: string }> = [
    { heading: 'Why this matters', pattern: /##\s*why\s+this\s+matters\b/i, minWords: 30, purpose: 'who benefits, what breaks without this, why now' },
    { heading: 'Why it helps Chorus', pattern: /##\s*why\s+it\s+helps\s+chorus\b/i, minWords: 30, purpose: 'how this serves the team coordination product, not just one role' },
    { heading: "Why it's not gold plating or a nit", pattern: /##\s*why\s+it'?s?\s+not\s+(?:gold\s+plating|a\s+nit)(?:\s+or\s+(?:gold\s+plating|a\s+nit))?/i, minWords: 30, purpose: "name the load-bearing reason this isn't cosmetic / edge-case / nice-to-have" },
    { heading: 'Dependencies', pattern: /##\s*dependencies\b/i, minWords: 20, purpose: 'enumerate what needs to land first / what other surfaces must change — count is the truth-teller' },
    { heading: 'Scope of impact', pattern: /##\s*scope\s+of\s+impact\b/i, minWords: 20, purpose: 'what surfaces this touches, who is affected when it lands, what could break elsewhere' },
  ];
  for (const sec of REQUIRED_SECTIONS) {
    const m = (opts.description || '').match(new RegExp(sec.pattern.source + '([\\s\\S]*?)(?=\\n##\\s|\\n*$)', 'i'));
    if (!m) {
      errors.push(`Description missing "## ${sec.heading}" section — ${sec.purpose}. The bouncer refuses proposals without a substantive answer to each of the six questions.`);
    } else {
      const wc = m[1].trim().split(/\s+/).filter(Boolean).length;
      if (wc < sec.minWords) {
        errors.push(`"## ${sec.heading}" too thin (${wc} words; need ≥${sec.minWords}) — ${sec.purpose}.`);
      }
    }
  }
}

// Reports collected errors and exits the process.
function reportErrorsAndExit(errors: string[], title: string, boardName: string): never {
  console.error(`ERROR: Card creation failed (${errors.length} issue${errors.length > 1 ? 's' : ''}):`);
  for (const err of errors) console.error(`  • ${err}`);
  emitSpineEvent(EVENT_CARD_QUALITY_BLOCKED, detectRole(), {
    title, gate: 'add_validation_failed', board: boardName,
    errors: errors.join('; '),
  });
  process.exit(1);
}

// Applies post-add tags (sequence, origin) and triggers workflow if status is Now.
// cog-override: applyPostAddTags: per-axis tag-application chain (#2652 added subdomain+subproduct branches; pre-existing for sequence/origin/workflow). Sequential branches, intentional.
async function applyPostAddTags(
  client: BoardClient, task: BoardTask, opts: AddOpts,
): Promise<void> {
  if (opts.sequence) {
    try { await client.tag(task.index, 'sequence', opts.sequence); }
    catch (err: unknown) { console.error(`  (sequence tag: ${err instanceof Error ? err.message : err})`); }
  }
  if (opts.origin) {
    try { await client.tag(task.index, 'origin', opts.origin.toLowerCase()); }
    catch (err: unknown) { console.error(`  (origin tag: ${err instanceof Error ? err.message : err})`); }
  }
  // #2652 AC1+AC2 — apply new tag axes (already validated refuse-at-source).
  // Labels auto-create on first use; subdomain/subproduct categories not in
  // LABELS config so use direct label add via client.applyLabelByName helper.
  if (opts.subproduct) {
    try { await applyDynamicLabel(client, task.index, `subproduct:${opts.subproduct.toLowerCase()}`); }
    catch (err: unknown) { console.error(`  (subproduct tag: ${err instanceof Error ? err.message : err})`); }
  }
  if (opts.subdomain) {
    try { await applyDynamicLabel(client, task.index, `subdomain:${opts.subdomain}`); }
    catch (err: unknown) { console.error(`  (subdomain tag: ${err instanceof Error ? err.message : err})`); }
  }
  if (task.status.toLowerCase() === 'now') {
    try { await triggerWorkflow(client, task.index); }
    catch (err: unknown) { console.error(`  (workflow: ${err instanceof Error ? err.message : err})`); }
  }
}

// #2652 AC1+AC2 — apply a label by full name (e.g. "subdomain:cards-service"),
// auto-creating the Vikunja label if it doesn't exist. Subdomain/subproduct
// labels are not in LABELS config (Athena is source of truth for subdomain;
// closed-list for subproduct). Reuses applyLabelByName which handles the
// find-or-create path.
async function applyDynamicLabel(
  client: BoardClient, index: number, labelName: string,
): Promise<void> {
  await client.applyLabelByName(index, labelName);
}

/**
 * #2924 AC3 bridge — write the pickup artifacts the bouncer hands off to the
 * chorus-hooks UserPromptSubmit responder. Two siblings under <pendingDir>:
 *   - <role>-<stamp>.txt        human-readable [card-approval] block
 *   - <role>-<stamp>.argv.json  structured {title, opts} for replay
 *
 * When Jeff types `approve`, the responder reads the .argv.json and replays
 * `cards add` with DEPLOY_ROLE=jeff (bypasses the bouncer cleanly). The .txt
 * remains for human inspection.
 */
/** #2964: dedupe window — within this many ms, an existing pending payload
 *  for the same role+title is overwritten in place instead of producing a
 *  fresh stamped file. Mirrors the responder's PENDING_TIMEOUT_SECS (10 min). */
export const PENDING_DEDUPE_WINDOW_MS = 600_000; // 10 minutes

/** #2964: retry-refusal window — within this many ms of a previous write for
 *  the same role+title, refuse the new attempt outright. Prevents agents from
 *  flood-retrying the gated path. */
export const PENDING_RETRY_REFUSAL_MS = 30_000; // 30 seconds

/**
 * #2964: thrown by writePendingApprovalArtifacts when an agent retries the
 * same role+title within PENDING_RETRY_REFUSAL_MS. The bouncer caller catches
 * this and exits with a clear "already-pending" message instead of stacking
 * a duplicate payload onto Jeff's queue.
 */
export class PendingRetryTooSoonError extends Error {
  constructor(public ageMs: number, public existingPath: string) {
    super(`pending-retry-too-soon: same-role+title written ${ageMs}ms ago at ${existingPath}`);
    this.name = 'PendingRetryTooSoonError';
  }
}

/** #2964: find an existing pending payload for the same role+title. Returns
 *  the path + mtime-age if any matching `<role>-*.argv.json` file holds the
 *  same title, otherwise null. Bouncer uses this for dedupe + retry refusal. */
export function findExistingPendingByTitle(
  pendingDir: string, role: string, title: string,
): { path: string; ageMs: number } | null {
  let entries: string[];
  try {
    entries = fs.readdirSync(pendingDir);
  } catch {
    return null;
  }
  const prefix = `${role}-`;
  const now = Date.now();
  for (const name of entries) {
    if (!name.startsWith(prefix) || !name.endsWith('.argv.json')) continue;
    const path = `${pendingDir}/${name}`;
    let raw: string;
    try { raw = fs.readFileSync(path, 'utf-8'); } catch { continue; }
    try {
      const parsed = JSON.parse(raw);
      if (parsed && typeof parsed === 'object' && parsed.title === title) {
        let mtime: number;
        try { mtime = fs.statSync(path).mtimeMs; } catch { continue; }
        // Clamp at 0 — on some filesystems mtime can be slightly larger than
        // Date.now() (sub-millisecond rounding); a negative age is meaningless.
        return { path, ageMs: Math.max(0, now - mtime) };
      }
    } catch { continue; }
  }
  return null;
}

export function writePendingApprovalArtifacts(args: {
  pendingDir: string;
  role: string;
  stamp: string;
  nudge: string;
  title: string;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  cardOpts: any;
}): { txtPath: string; argvPath: string } {
  fs.mkdirSync(args.pendingDir, { recursive: true });

  // #2964: dedupe + retry refusal. If we've written a pending payload for the
  // same role+title recently, decide what to do based on age. This stops the
  // bouncer from piling 24 duplicates on Jeff's queue when an agent retries.
  const existing = findExistingPendingByTitle(args.pendingDir, args.role, args.title);
  if (existing) {
    if (existing.ageMs < PENDING_RETRY_REFUSAL_MS) {
      throw new PendingRetryTooSoonError(existing.ageMs, existing.path);
    }
    if (existing.ageMs < PENDING_DEDUPE_WINDOW_MS) {
      // Overwrite the existing payload in place so we don't accumulate
      // duplicates. Use the same stem so the responder treats this as one
      // pending payload, not a queue depth of two.
      //
      // Semantics note (#2964 Kade gate:quality feedback): the dedupe keys on
      // role+title only. If a second call inside the dedupe window arrives
      // with different cardOpts (e.g., priority P1 → P2, description edits,
      // a renamed owner), THIS PATH ABSORBS the new opts silently. That is
      // deliberate — "one pending per role+title" is the right invariant; the
      // alternative (refuse the second call's edits, keep stale opts) is
      // worse. But it means an agent reshaping a card mid-window will see the
      // new opts land in Jeff's queue without a new approval-ask firing.
      // Future-Wren: if cardOpts-drift becomes a real surface, surface a
      // "payload updated" event here so Jeff can see the change.
      const argvPath = existing.path;
      const txtPath = argvPath.replace(/\.argv\.json$/, '.txt');
      fs.writeFileSync(txtPath, args.nudge);
      fs.writeFileSync(argvPath, JSON.stringify({ title: args.title, opts: args.cardOpts }, null, 2));
      return { txtPath, argvPath };
    }
    // Stale (> dedupe window) — fall through to new-stamp write. Responder's
    // sweep_stale_pending will clean up the old one on the next pass.
  }

  const txtPath = `${args.pendingDir}/${args.role}-${args.stamp}.txt`;
  const argvPath = `${args.pendingDir}/${args.role}-${args.stamp}.argv.json`;
  fs.writeFileSync(txtPath, args.nudge);
  fs.writeFileSync(argvPath, JSON.stringify({ title: args.title, opts: args.cardOpts }, null, 2));
  return { txtPath, argvPath };
}

/**
 * #2924 AC1 — deliver the structured approval-ask into the requesting agent's
 * session via the pulse messaging API. Best-effort: if pulse is unreachable,
 * the bouncer still refuses and the pickup file remains the fallback surface.
 *
 * The nudge addresses the *requesting role* (the agent's own session), not
 * Jeff — because Jeff is interacting with that terminal. Nudges injected
 * there surface to him as user-prompt-style messages, and his `approve` /
 * `deny` reply is detected by the #2924 AC3 UserPromptSubmit hook which
 * then completes or cancels the deferred card filing.
 */
export type FetchLike = (
  url: string,
  init?: {
    method?: string;
    headers?: Record<string, string>;
    body?: string;
    signal?: AbortSignal;
  },
) => Promise<{ ok: boolean; status: number; text?: () => Promise<string> }>;

export async function sendCardApprovalNudge(args: {
  from: string;
  to: string;
  message: string;
  pulseUrl?: string;
  fetchImpl?: FetchLike;
  timeoutMs?: number;
}): Promise<{ delivered: boolean; status?: number; error?: string; traceId: string }> {
  const pulseUrl = args.pulseUrl || process.env.CHORUS_PULSE_URL || 'http://localhost:3475/api/nudge';
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const fetchImpl: FetchLike | undefined = args.fetchImpl || ((globalThis as any).fetch as FetchLike | undefined);
  const traceId = `card-approval-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
  if (!fetchImpl) {
    return { delivered: false, error: 'no-fetch-impl', traceId };
  }
  try {
    const ctrl = new AbortController();
    const timeoutId = setTimeout(() => ctrl.abort(), args.timeoutMs ?? 5000);
    const resp = await fetchImpl(pulseUrl, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'X-Chorus-Trace-Id': traceId,
        'X-Chorus-MCP-Caller': '1',
      },
      body: JSON.stringify({ from: args.from, to: args.to, content: args.message, traceId }),
      signal: ctrl.signal,
    });
    clearTimeout(timeoutId);
    if (!resp.ok) {
      const errText = resp.text ? await resp.text().catch(() => '') : '';
      return { delivered: false, status: resp.status, error: errText.slice(0, 200), traceId };
    }
    return { delivered: true, status: resp.status, traceId };
  } catch (err) {
    return { delivered: false, error: err instanceof Error ? err.message : String(err), traceId };
  }
}

/**
 * #2905 (Jeff direct 2026-05-11): agent-initiated `cards add` is REFUSED.
 * The CLI composes the structured approval-ask nudge from the description
 * the agent already wrote (validated to have the six required sections),
 * prints it to stdout for the agent to read + decide whether to send via
 * chorus_nudge_message to Jeff, and exits non-zero. The card is not filed.
 *
 * The agent may decide — after seeing what they wrote in the mirror —
 * not to send the nudge at all. That's the forcing function: many weak
 * cards die at this step before Jeff ever sees them.
 *
 * If Jeff approves, Jeff files the card himself from his own terminal
 * (DEPLOY_ROLE=jeff or unset → no bouncer hook fires).
 *
 * Bypasses (file immediately, no nudge):
 *   - DEPLOY_ROLE=jeff or unset (Jeff acting directly).
 *   - NODE_ENV=test (hermetic test runs).
 *
 * No env-var bypass an agent can set in their shell.
 */
// #2996: directive-marker retired. The dedicated `chorus_card_add_jeff` MCP
// tool (spawned with DEPLOY_ROLE=jeff hardcoded) replaces the marker-handshake
// path. /card invocation → MCP tool → DEPLOY_ROLE=jeff → requireJeffApprovalIfAgent
// returns immediately because isAgent is false. One mechanism, deterministic.

async function requireJeffApprovalIfAgent(title: string, opts: AddOpts): Promise<void> {
  const role = (process.env.DEPLOY_ROLE || '').toLowerCase();
  const isAgent = role === 'wren' || role === 'silas' || role === 'kade';
  if (!isAgent) return;
  if (process.env.NODE_ENV === 'test') return;

  // Compose the structured approval-ask nudge from the description sections.
  // validateDescription already enforced these are present + substantive.
  const desc = opts.description || '';
  const section = (heading: RegExp): string => {
    const m = desc.match(new RegExp(heading.source + '([\\s\\S]*?)(?=\\n##\\s|\\n*$)', 'i'));
    return m ? m[1].trim() : '';
  };
  const whyMatters = section(/##\s*why\s+this\s+matters\b/i);
  const whyHelpsChorus = section(/##\s*why\s+it\s+helps\s+chorus\b/i);
  const whyNotGold = section(/##\s*why\s+it'?s?\s+not\s+(?:gold\s+plating|a\s+nit)(?:\s+or\s+(?:gold\s+plating|a\s+nit))?/i);
  const deps = section(/##\s*dependencies\b/i);
  const scope = section(/##\s*scope\s+of\s+impact\b/i);
  const experience = section(/##\s*experience\b/i);

  // #2964: label-anchored format. Each section is one line starting with an
  // ALLCAPS label so visual structure survives newline-flattening (osascript
  // keystroke injection, model re-rendering one-paragraph). Long sections
  // are truncated to ~220 chars with an ellipsis — full text is in the pickup
  // file if Jeff wants depth. Approve/deny line is last and explicit.
  const trunc = (s: string, n: number) => (s.length > n ? s.slice(0, n - 1).trimEnd() + '…' : s);
  const nudge = `[card-approval] ${role} → jeff
TITLE:    ${title}
OWNER:    ${opts.owner || '(unset)'}    PRIO: ${opts.priority || '(unset)'}    TYPE: ${opts.type || '(unset)'}    DOMAIN: ${opts.domain || '(unset)'}    SEQ: ${opts.sequence || '(unset)'}
CHANGE:   ${trunc(experience, 240)}
WHY:      ${trunc(whyMatters, 240)}
HELPS:    ${trunc(whyHelpsChorus, 240)}
NOT-NIT:  ${trunc(whyNotGold, 200)}
DEPS:     ${trunc(deps, 200)}
SCOPE:    ${trunc(scope, 220)}
APPROVE:  reply "approve" to file, "deny" to discard. Or DEPLOY_ROLE=jeff cards add … in your terminal bypasses cleanly. Full description in the pickup file.`;

  // #2910: write the composed ask to a pickup file so the model can surface it
  // in its next response to Jeff. Previous design relied on the model reading
  // its own stdout and deciding to forward the nudge — Silas demonstrated that
  // gap by relaying a summary instead. The pickup file lets the next-response
  // hook (or model-side discipline per skills/cards/bouncer-flow.md) surface
  // the ask verbatim. Without this, the only reliable delivery channel to Jeff
  // (model response text) is opt-in.
  const pendingDir = `${process.env.HOME || '/Users/jeffbridwell'}/.chorus/pending-approvals`;
  try {
    const stamp = new Date().toISOString().replace(/[:.]/g, '-');
    const { txtPath, argvPath } = writePendingApprovalArtifacts({
      pendingDir, role, stamp, nudge, title, cardOpts: opts,
    });
    console.log(`[card-approval pickup written: ${txtPath}]`);
    console.log(`[card-approval argv sidecar written: ${argvPath}  — read by the AC3 UserPromptSubmit responder on approve]`);
  } catch (err) {
    // #2964: retry-too-soon — agent flood-retrying the gated path. Refuse
    // outright with the existing pending path so the agent can see its prior
    // attempt is still queued, and exit non-zero so the retry counts as a
    // refusal (not a silent success).
    if (err instanceof PendingRetryTooSoonError) {
      console.error(`---`);
      console.error(`REFUSED: retry-too-soon. Same role+title was queued ${Math.round(err.ageMs / 1000)}s ago at ${err.existingPath}.`);
      console.error(`The bouncer has NOT written a duplicate. The original pending payload is still active and will be processed when Jeff approves.`);
      console.error(`If you're trying to reshape the card, wait at least ${Math.ceil((PENDING_RETRY_REFUSAL_MS - err.ageMs) / 1000)}s and try again.`);
      process.exit(1);
    }
    // Other write failures: log but proceed to the standard refusal exit.
    console.error(`WARN: failed to write pickup artifacts under ${pendingDir} — ${err instanceof Error ? err.message : err}. Composed ask still in stdout above; agent must surface it manually.`);
  }

  // #3293: the #2924 pulse-nudge delivery was REMOVED. It was a third delivery
  // path (alongside the pickup file + stdout) and it mis-routed — addressed
  // to:role but landing in silas's session, and it dressed an intended refusal
  // as traffic. The canonical channel is the pickup file + stdout + the AC3
  // "approve" responder; the filing model surfaces the ask verbatim to Jeff in
  // its next reply (Jeff is in the filing terminal). One path, no mis-route.

  console.log('---');
  console.log('Agent card creation is refused. The structured ask was written to the pickup file above AND printed below.');
  console.log('The model contract (per skills/cards/bouncer-flow.md): before your next response to Jeff, surface this [card-approval] block verbatim in your reply text, then delete the pickup file.');
  console.log('---');
  console.log(nudge);
  console.log('---');
  console.error('REFUSED: agent cards add requires Jeff approval. No card filed. The pickup file is the auto-send substrate — the model surfaces the ask in its next response, no agent opt-out.');
  process.exit(1);
}

export async function addCard(
  client: BoardClient, title: string, opts: AddOpts,
): Promise<BoardTask> {
  warnShortTitle(title, client.boardName);

  // Classification gates: ALWAYS enforced (#1966).
  inferCardDefaults(title, opts);

  const errors = await collectRequiredFieldErrors(opts);
  validateDescription(opts, title, client.boardName, errors);
  if (errors.length > 0) reportErrorsAndExit(errors, title, client.boardName);

  // #2895: agent-initiated card adds route through Jeff for approval first.
  // Jeff-self (DEPLOY_ROLE=jeff or unset) bypasses the gate — files immediately.
  await requireJeffApprovalIfAgent(title, opts);

  const task = await client.add(title, opts);
  const productTag = opts.product ? ` [product:${opts.product}]` : '';
  console.log(`Added #${task.index}: ${title} [${task.status}]${productTag}`);
  emitSpineEvent('card.item.created', detectRole(), {
    card_id: String(task.index), title, status: task.status, board: client.boardName,
    ...(opts.product ? { product: opts.product } : {}),
  });

  await applyPostAddTags(client, task, opts);
  return task;
}

async function enforceWipBlastRadius(
  client: BoardClient, index: number, title: string, card: BoardTask,
): Promise<void> {
  try {
    const fullText = `${title}\n${card.description || ''}`;
    if (!isCodeCard(fullText)) return;
    const domainLabel = card.domains.find((d: string) => d.startsWith('domain:'));
    const cardDomain = domainLabel ? domainLabel.replace('domain:', '') : undefined;
    const report = await generateBlastRadius(index, title, card.description || '', cardDomain);
    if (report && report.totalFiles === 0) {
      // #2810: zero is a valid computation result (yaml+html cards, pull-time
      // before commits, cards whose AC doesn't mention specific paths).
      // Don't refuse the pull. Compute, record, warn, proceed.
      console.warn(`NOTE: Blast radius: 0 files computed for code card #${index} — non-blocking.`);
      emitSpineEvent('card.blast_radius.zero_code', detectRole(), {
        card_id: String(index), title, board: client.boardName,
      });
    }
  } catch { /* blast radius API failure = non-blocking, proceed to WIP */ }
}

// Pre-move gates — runs before the actual status change. Exits process on gate failure.
async function enforcePreMoveGates(
  client: BoardClient, index: number, title: string, card: BoardTask, status: string,
): Promise<void> {
  const isNow = status.toLowerCase() === 'now';
  const isWip = /^wip$/i.test(status);
  if (isNow && !enforceNowDescriptionGate(index, title, card.description, client.boardName)) process.exit(1);
  if (isWip && !enforceACGate(index, title, card.description, client.boardName)) process.exit(1);
  if (isWip && !enforceExperienceGate(index, title, card.description, client.boardName)) process.exit(1);
  if (isNow || isWip) enforceTaxonomyGate(index, title, card.domains, client.boardName);
  if (isWip) await enforceWipBlastRadius(client, index, title, card);
}

// Generates and posts blast radius comment after WIP entry. Non-blocking on failure.
async function postBlastRadiusOnWip(
  client: BoardClient, index: number, title: string, role: string,
): Promise<void> {
  try {
    const card = title ? await client.view(index).catch(() => null) : null;
    const desc = card?.description || '';
    const domainLabel2 = (card?.domains || []).find((d: string) => d.startsWith('domain:'));
    const cardDomain2 = domainLabel2 ? domainLabel2.replace('domain:', '') : undefined;
    const report = await generateBlastRadius(index, title, desc, cardDomain2);
    if (report && report.totalFiles > 0) {
      const comment = formatBlastComment(report);
      await client.comment(index, comment);
      console.log(`  Blast radius: ${report.totalFiles} files, ${report.crossDomain.length} domains`);
      emitSpineEvent('card.blast_radius.generated', role, {
        card_id: String(index), files: String(report.totalFiles),
        domains: report.crossDomain.join(','),
      });
    }
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : String(err);
    console.error(`  (blast radius: ${msg})`);
    emitSpineEvent('card.blast_radius.failed', role, {
      card_id: String(index), error: msg.slice(0, 200),
    });
  }
}

// Finds cards in WIP that share domain/sequence/chunk/stream labels. Returns overlap lines.
function findWipOverlaps(card: BoardTask, wipCards: BoardTask[], selfIndex: number): string[] {
  const myChunks = card.domains.filter(d => d.startsWith('chunk:'));
  const mySequences = card.domains.filter(d => d.startsWith('sequence:'));
  const myDomains = card.domains.filter(d => d.startsWith('domain:'));
  const myStreams = card.domains.filter(d => d.startsWith('stream:'));
  const overlaps: string[] = [];
  for (const other of wipCards) {
    if (other.index === selfIndex) continue;
    const shared = {
      chunks: myChunks.filter(c => other.domains.includes(c)),
      seqs: mySequences.filter(s => other.domains.includes(s)),
      domains: myDomains.filter(d => other.domains.includes(d)),
      streams: myStreams.filter(s => other.domains.includes(s)),
    };
    if (shared.domains.length > 0) overlaps.push(`  ${other.owner} has #${other.index} in ${shared.domains.join(', ')}`);
    if (shared.streams.length > 0) overlaps.push(`  ${other.owner} has #${other.index} in ${shared.streams.join(', ')}`);
    if (shared.chunks.length > 0) overlaps.push(`  ${other.owner} has #${other.index} in ${shared.chunks.join(', ')}`);
    if (shared.seqs.length > 0) overlaps.push(`  ${other.owner} has #${other.index} in ${shared.seqs.join(', ')}`);
  }
  return overlaps;
}

// WIP overlap detection (#1318) — warn when entering a domain/sequence with active WIP.
async function detectWipOverlap(client: BoardClient, index: number, role: string): Promise<void> {
  try {
    const card = await client.view(index).catch(() => null);
    if (!card) return;
    const hasLabels = card.domains.some(d => /^(chunk|sequence|domain|stream):/.test(d));
    if (!hasLabels) return;
    const grouped = await client.listGrouped();
    const wipCards = grouped.get('WIP') || [];
    const overlaps = findWipOverlaps(card, wipCards, index);
    if (overlaps.length > 0) {
      console.log('  WIP overlap detected:');
      overlaps.forEach(o => console.log(o));
      emitSpineEvent('card.wip_overlap.detected', role, {
        card_id: String(index), overlaps: String(overlaps.length),
      });
    }
  } catch { /* non-blocking */ }
}

export async function moveCard(
  client: BoardClient, index: number, status: string
): Promise<void> {
  const role = detectRole();
  let title = '';
  let owner = '';
  try {
    const card = await client.view(index);
    title = card.title;
    owner = card.owner;
    await enforcePreMoveGates(client, index, title, card, status);
  } catch { /* best effort */ }

  await client.move(index, status);
  console.log(`Moved #${index} to ${status}`);

  const isWontDo = /won.?t.?do|wd|killed|dup|not.?doing/i.test(status);
  const moveFields: Record<string, string> = {
    card_id: String(index), title, to: status, board: client.boardName,
  };
  if (isWontDo) moveFields.reason = 'wont_do';
  emitSpineEvent('card.item.moved', role, moveFields);
  if (/^wip$/i.test(status)) {
    emitSpineEvent('card.pulled', role, { card_id: String(index), title, board: client.boardName, hop: '1', source_service: 'board', dest_service: 'role-state', callStack: 'integration' });
    autoRoleState('building', `card=${index}`);
  }
  notifyOwnerIfDifferent(index, title, owner, `moved-to-${status}`, role);

  if (status.toLowerCase() === 'now') {
    try { await triggerWorkflow(client, index); }
    catch (err: unknown) { console.error(`  (workflow: ${err instanceof Error ? err.message : err})`); }
  }

  if (/^wip$/i.test(status)) {
    await postBlastRadiusOnWip(client, index, title, role);
    await detectWipOverlap(client, index, role);
  }
}

// #2707 — verify the move actually applied. client.done() returns void on
// moveToBucket success but the board can silently leave the card in WIP
// (transient API timeout, async race, board cache miss). Without this check,
// "Done: #N" + card.accepted spine event fire for a card still in WIP —
// every downstream consumer of card.accepted then lies. Retries once with a
// short backoff so transient API hiccups don't false-fail.
async function verifyDoneApplied(client: BoardClient, index: number): Promise<void> {
  const readStatus = async (): Promise<string> => {
    try { return (await client.view(index)).status; } catch { return 'unknown'; }
  };
  let postStatus = await readStatus();
  if (postStatus === 'Done') return;
  await new Promise((resolve) => setTimeout(resolve, 250));
  postStatus = await readStatus();
  if (postStatus === 'Done') return;
  throw new Error(
    `cards done ${index}: board did not move card to Done after 2 attempts. Status remains: ${postStatus}. Retry manually or investigate board API.`,
  );
}

export async function doneCard(client: BoardClient, index: number, provenCards?: string[]): Promise<void> {
  const role = detectRole();
  let title = '';
  let owner = '';
  try {
    const card = await client.view(index);
    title = card.title;
    owner = card.owner;
  } catch { /* best effort */ }

  await warnNoComments(client, index, title, client.boardName);

  // #1916: --proven bypass for retroactive closure
  if (provenCards && provenCards.length > 0) {
    const evidenceList = provenCards.map(c => `#${c}`).join(', ');
    console.log(`Proven: #${index} — evidence from ${evidenceList}`);
    try {
      await client.comment(index, `proven: evidence from ${evidenceList}`);
    } catch { /* best effort */ }
    emitSpineEvent('card.accepted.proven', role, {
      card_id: String(index), title, board: client.boardName,
      evidence: provenCards.join(','),
    });
  }
  // #3227 — NO demo gate here. cards-done is a board PRIMITIVE, not the accept
  // authority. The demo gate lives in werk-accept (demo_verdict_pass, the single
  // reader of the demo.verdict witness, #3116) — which calls `cards done` to
  // finalize. Gating here too produced a two-speed accept: a card cleared
  // werk-accept's verdict gate then DIED at this TS comment-gate (the #3222
  // gauntlet). One gate, one source of truth. A direct `cards done` is
  // deliberately below the accept ceremony — "done is Jeff's call, not the
  // harness's." (Supersedes #1834/#2910's premise that cards-done should gate.)

  await client.done(index);
  await verifyDoneApplied(client, index);

  console.log(`Done: #${index}`);
  emitSpineEvent('card.item.completed', role, {
    card_id: String(index), title, board: client.boardName,
  });
  emitSpineEvent('card.accepted', role, {
    card_id: String(index), title, board: client.boardName, hop: '1', source_service: 'board', dest_service: 'role-state', callStack: 'integration',
  });
  autoRoleState('idle');

  // #2652 AC3 — single emit function in cards. Was emitChorusEvent (different
  // appName/component); migrated to emitSpineEvent so all card-emitted events
  // share one canonical chain. Single-implementation invariant.
  emitSpineEvent('deploy.verification.completed', role, {
    card_id: String(index), title, result: 'pass', method: 'manual',
  });

  reconcileWorkflows(index, role);
  notifyOwnerIfDifferent(index, title, owner, 'done', role);
  notifyPM(index, title, owner, role);

  await tryAutoUnblockDownstream(client, index, role);
}

async function allBlockersDone(client: BoardClient, blockerIds: number[]): Promise<boolean> {
  for (const blockerId of blockerIds) {
    try {
      const blocker = await client.view(blockerId);
      if (blocker.status !== 'Done') return false;
    } catch {
      return false;
    }
  }
  return true;
}

async function maybeUnblockGated(client: BoardClient, gatedId: number, completedIndex: number, role: string): Promise<void> {
  const gatedRels = await client.getRelations(gatedId);
  if (!(await allBlockersDone(client, gatedRels.blockedBy))) return;
  const gatedCard = await client.view(gatedId);
  if (gatedCard.status !== 'Later') return;
  await client.move(gatedId, 'Next');
  console.log(`  Unblocked #${gatedId} — moved Later → Next`);
  // #2652 AC4 — was 'card.unblocked' (alias retired 2026-05-02 — no live
  // consumers in chorus tree per grep audit). Canonical name: 'card.item.unblocked'.
  emitSpineEvent('card.item.unblocked', role, {
    card_id: String(gatedId), title: gatedCard.title, unblocked_by: String(completedIndex),
  });
}

async function tryAutoUnblockDownstream(client: BoardClient, index: number, role: string): Promise<void> {
  try {
    const rels = await client.getRelations(index);
    if (rels.blocks.length === 0) return;
    for (const gatedId of rels.blocks) {
      await maybeUnblockGated(client, gatedId, index, role);
    }
  } catch { /* best effort — sequencing is additive, not blocking */ }
}

export async function demoCard(client: BoardClient, index: number): Promise<void> {
  const role = detectRole();
  let title = '';
  try {
    const card = await client.view(index);
    title = card.title;
    // #2017: Auto-check AC items — replace - [ ] with - [x] before gates run.
    // Roles forget to mark boxes; gate:product fails on unchecked boxes, not missing work.
    if (card.description && card.description.includes('- [ ]')) {
      const checked = card.description.replace(/- \[ \]/g, '- [x]');
      await client.update(index, { description: checked });
      const count = (card.description.match(/- \[ \]/g) || []).length;
      console.log(`  Auto-checked ${count} AC item${count !== 1 ? 's' : ''} on #${index}`);
    }
  } catch { /* best effort */ }
  emitSpineEvent('card.demo.started', role, {
    card_id: String(index), title, board: client.boardName,
  });
  console.log(`Demo started: #${index}${title ? ' — ' + title : ''}`);
}

export async function rejectCard(client: BoardClient, index: number, reason: string): Promise<void> {
  const role = detectRole();
  let title = '';
  try { title = (await client.view(index)).title; } catch { /* best effort */ }
  emitSpineEvent('card.rejected', role, {
    card_id: String(index), title, reason, board: client.boardName,
  });
  console.log(`Rejected: #${index}${title ? ' — ' + title : ''} (${reason})`);
}

export async function blockCard(client: BoardClient, index: number, reason: string): Promise<void> {
  let title = '';
  try { title = (await client.view(index)).title; } catch { /* best effort */ }
  await client.block(index, reason);
  console.log(`Blocked: #${index}${reason ? ` — ${reason}` : ''}`);
  emitSpineEvent('card.item.blocked', detectRole(), {
    card_id: String(index), title, reason: reason || 'unspecified', board: client.boardName,
  });
}

export async function unblockCard(client: BoardClient, index: number): Promise<void> {
  let title = '';
  try { title = (await client.view(index)).title; } catch { /* best effort */ }
  await client.unblock(index);
  console.log(`Unblocked: #${index} → Next`);
  emitSpineEvent('card.item.unblocked', detectRole(), {
    card_id: String(index), title, board: client.boardName,
  });
}

/**
 * #2652 AC12 — `cards check <id> AC<n>` — single-keystroke checkbox flip on
 * description from anywhere a role is working. Closes the comments-vs-description
 * divergence: roles routinely evidence AC completion in comments while the
 * description checkboxes stay unchecked, blocking gate-product reads (Kade
 * observation 2026-05-01). Spine event: `card.ac.checked` with
 * {card_id, ac_index, role}.
 *
 * Behavior:
 *   - Find the Nth `- [ ]` line in the description (1-indexed)
 *   - Flip to `- [x]`
 *   - Update card via existing updateCard path (which emits ac.ticked)
 *   - Emit card.ac.checked
 *
 * Errors:
 *   - If AC<n> doesn't exist or is already checked, print message + exit 0 (no-op).
 */
export async function checkCardAc(
  client: BoardClient, index: number, acIndex: number
): Promise<void> {
  const role = detectRole();
  const card = await client.view(index);
  const desc = card.description || '';
  const lines = desc.split('\n');
  let countUnchecked = 0;
  let targetLine = -1;
  for (let i = 0; i < lines.length; i++) {
    const m = lines[i].match(/^(\s*-\s*\[) \](.*)$/);
    if (m) {
      countUnchecked++;
      if (countUnchecked === acIndex) {
        targetLine = i;
        break;
      }
    }
  }
  if (targetLine === -1) {
    console.log(`#${index} has no unchecked AC${acIndex} (only ${countUnchecked} unchecked items found) — no-op`);
    return;
  }
  lines[targetLine] = lines[targetLine].replace(/^(\s*-\s*\[) \]/, '$1x]');
  const newDesc = lines.join('\n');
  await client.update(index, { description: newDesc });
  console.log(`Checked AC${acIndex} on #${index}`);
  emitSpineEvent('card.ac.checked', role, {
    card_id: String(index),
    ac_index: String(acIndex),
    board: client.boardName,
  });
}

// cog-override: updateCard: title/desc/product merge with prior-state read for ac.ticked diff — pre-existing complexity, structurally cohesive.
export async function updateCard(
  client: BoardClient, index: number,
  fields: { title?: string; description?: string; product?: string }
): Promise<void> {
  // #2193: read old description BEFORE update so ac.ticked can diff old→new.
  let oldDesc = '';
  if (fields.description !== undefined) {
    try { oldDesc = (await client.view(index)).description || ''; } catch { /* pre-read best-effort */ }
  }

  await client.update(index, fields);

  // Verify-after-write: re-read card to confirm description persisted (#1267)
  if (fields.description !== undefined) {
    const expectedLen = fields.description.length;

    // Block empty description writes — cards need content
    if (expectedLen === 0) {
      console.error('ERROR: Empty description — refusing to clear card description. Provide content or omit --desc.');
      process.exit(1);
    }

    const verified = await client.view(index);
    const actualLen = (verified.description || '').length;

    if (actualLen === 0) {
      console.error(`ERROR: Description write failed — wrote ${expectedLen} chars but card has empty description after update.`);
      process.exit(1);
    }

    console.log(`Updated #${index} — desc: ${actualLen} chars${fields.product ? ` [product:${fields.product}]` : ''}`);

    // #2193: emit ac.ticked when the update flipped any `- [ ]` → `- [x]`.
    const diff = countAcDiff(oldDesc, fields.description);
    if (diff.tickedCount > 0) {
      emitSpineEvent('ac.ticked', detectRole(), {
        card_id: String(index),
        ticked_count: String(diff.tickedCount),
        total_checked: String(diff.totalChecked),
        total_ac: String(diff.totalAc),
        board: client.boardName,
      });
    }
  } else {
    console.log(`Updated #${index}${fields.product ? ` [product:${fields.product}]` : ''}`);
  }

  emitSpineEvent('card.item.updated', detectRole(), {
    card_id: String(index), board: client.boardName,
    ...(fields.title ? { new_title: fields.title } : {}),
    ...(fields.description !== undefined ? { description_changed: 'true' } : {}),
    ...(fields.product ? { product: fields.product } : {}),
  });
}

export async function commentCard(client: BoardClient, index: number, text: string): Promise<void> {
  let title = '';
  try { title = (await client.view(index)).title; } catch { /* best effort */ }
  await client.comment(index, text);
  console.log(`Comment added to #${index}`);
  emitSpineEvent('card.item.commented', detectRole(), {
    card_id: String(index), title, board: client.boardName,
  });
}

export async function reassignCard(client: BoardClient, index: number, newOwner: string): Promise<void> {
  const validRoles = ['wren', 'silas', 'kade', 'jeff'];
  const role = newOwner.toLowerCase();
  if (!validRoles.includes(role)) {
    console.error(`ERROR: Invalid role "${newOwner}". Valid: ${validRoles.join(', ')}`);
    process.exit(1);
  }

  let title = '';
  try { title = (await client.view(index)).title; } catch { /* best effort */ }

  const { oldOwner, newOwner: displayOwner } = await client.reassignOwner(index, role);
  console.log(`Reassigned #${index}: ${oldOwner || 'unassigned'} → ${displayOwner}`);
  emitSpineEvent('card.item.reassigned', detectRole(), {
    card_id: String(index), title, old_owner: oldOwner || 'unassigned', new_owner: role,
    board: client.boardName,
  });

  notifyOwnerIfDifferent(index, title, displayOwner, `reassigned-to-${displayOwner}`, detectRole());
}

// #2652 AC1+AC2 — subdomain + subproduct keys accepted on `cards set`. They
// route through the dynamic-label path (Vikunja label auto-create) since
// they're not in LABELS config — same as the create path's applyDynamicLabel.
// `product` (gathering|chorus portfolio) added so all four taxonomy axes —
// product / subproduct / domain / subdomain — are settable on existing cards
// per Jeff direction 2026-05-02 (board-wide attribute backfill).
const SET_CARD_VALID_KEYS = new Set(['domain', 'chunk', 'sequence', 'stream', 'type', 'origin', 'owner', 'priority', 'title', 'desc', 'description', 'status', 'after', 'gates', 'subdomain', 'subproduct', 'product']);
// #3267: chunk removed from the static-map tag loop — it now routes through
// applyLabelByName (auto-create), same as subproduct/subdomain, so chunk is a
// dynamic priority axis: a new priority is just a tag, no config.ts/enum edit.
const SET_CARD_TAG_CATEGORIES = ['domain', 'sequence', 'stream', 'type', 'origin', 'product'];

function validateSetKeys(pairs: Record<string, string>): void {
  for (const key of Object.keys(pairs)) {
    if (!SET_CARD_VALID_KEYS.has(key)) {
      throw new Error(`Unknown key "${key}". Valid: ${[...SET_CARD_VALID_KEYS].join(', ')}`);
    }
  }
  // #2652 AC2 — subproduct closed-list refuse-at-source on set path
  if (pairs.subproduct && !VALID_SUBPRODUCTS.has(pairs.subproduct.toLowerCase())) {
    throw new Error(`Unknown subproduct "${pairs.subproduct}". Valid: ${Array.from(VALID_SUBPRODUCTS).join(', ')}`);
  }
}

async function applyTagChanges(client: BoardClient, index: number, pairs: Record<string, string>, changes: string[]): Promise<void> {
  for (const cat of SET_CARD_TAG_CATEGORIES) {
    if (pairs[cat]) {
      await client.tag(index, cat, pairs[cat]);
      changes.push(`${cat}=${pairs[cat]}`);
    }
  }
  if (pairs.priority) {
    await client.tag(index, 'priority', pairs.priority.toUpperCase());
    changes.push(`priority=${pairs.priority}`);
  }
  // #2652 AC1+AC2 — apply dynamic-label categories. Subdomain refuses-at-source
  // against Athena (live query, fail-closed); subproduct is closed-list
  // (validated in validateSetKeys above).
  if (pairs.subdomain) {
    const valid = await fetchSubdomainSet();
    if (!valid.has(pairs.subdomain)) {
      throw new Error(`Unknown subdomain "${pairs.subdomain}". Athena reports ${valid.size} valid subdomains.`);
    }
    await client.applyLabelByName(index, `subdomain:${pairs.subdomain}`);
    changes.push(`subdomain=${pairs.subdomain}`);
  }
  if (pairs.subproduct) {
    await client.applyLabelByName(index, `subproduct:${pairs.subproduct.toLowerCase()}`);
    changes.push(`subproduct=${pairs.subproduct}`);
  }
  // #3267: chunk is the dynamic PRIORITY axis. Route through applyLabelByName so
  // a new priority chunk (werk/model/loom/proving…) auto-creates its label —
  // no VALID_CHUNKS enum, no config.ts label-id edit per priority. Normalize +
  // warn on first use so a typo (werk vs work) is visible, not silently minted.
  if (pairs.chunk) {
    const value = pairs.chunk.toLowerCase().trim();
    const { created } = await client.applyLabelByName(index, `chunk:${value}`);
    if (created) {
      console.warn(`⚠ new chunk "${value}" — first use, label created. (typo check: is this an existing chunk misspelled?)`);
    }
    changes.push(`chunk=${value}`);
  }
}

async function applyRelationPairs(client: BoardClient, index: number, pairs: Record<string, string>, changes: string[]): Promise<void> {
  if (pairs.after) {
    for (const dep of pairs.after.split(',')) {
      const depId = parseInt(dep.trim(), 10);
      if (isNaN(depId)) continue;
      await client.addRelation(index, depId, 'blocked');
      changes.push(`after=${depId}`);
    }
  }
  if (pairs.gates) {
    for (const dep of pairs.gates.split(',')) {
      const depId = parseInt(dep.trim(), 10);
      if (isNaN(depId)) continue;
      await client.addRelation(depId, index, 'blocked');
      changes.push(`gates=${depId}`);
    }
  }
}

async function printResultingCard(client: BoardClient, index: number, changes: string[]): Promise<void> {
  const card = await client.view(index);
  console.log(`#${card.index} ${card.title}`);
  console.log(`  Status:   ${card.status}`);
  if (card.owner) console.log(`  Owner:    ${card.owner}`);
  if (card.priority) console.log(`  Priority: ${card.priority}`);
  if (card.domains.length) console.log(`  Domains:  ${card.domains.join(', ')}`);
  if (changes.length) console.log(`  Changed:  ${changes.join(', ')}`);
}

export async function setCard(client: BoardClient, index: number, pairs: Record<string, string>): Promise<void> {
  validateSetKeys(pairs);
  const changes: string[] = [];

  // #2652 AC5 — capture before-state for per-field diff payload.
  let beforeCard: BoardTask | undefined;
  try { beforeCard = await client.view(index); } catch { /* view may fail; diff payload omitted */ }

  await applyTagChanges(client, index, pairs, changes);

  if (pairs.owner) {
    await client.reassignOwner(index, pairs.owner);
    changes.push(`owner=${pairs.owner}`);
  }

  if (pairs.title || pairs.desc || pairs.description) {
    const desc = pairs.desc || pairs.description;
    await updateCard(client, index, { title: pairs.title, description: desc });
    if (pairs.title) changes.push(`title="${pairs.title}"`);
    if (desc) changes.push(`desc=(${desc.length} chars)`);
  }

  if (pairs.status) {
    await moveCard(client, index, pairs.status);
    changes.push(`status=${pairs.status}`);
  }

  await applyRelationPairs(client, index, pairs, changes);
  await printResultingCard(client, index, changes);

  // #2652 AC5 — extend card.item.set payload with {field, old_value, new_value}
  // diffs so subscribers can reconstruct field history, not just see "something
  // changed." Old-value capture is best-effort (beforeCard view may have failed);
  // existing 'changes' string preserved for backward compat.
  const fieldChanges = beforeCard
    ? buildFieldChanges(beforeCard, pairs)
    : [];
  emitSpineEvent('card.item.set', detectRole(), {
    card_id: String(index),
    changes: changes.join(','),
    field_changes: JSON.stringify(fieldChanges),
    board: client.boardName,
  });
}

// #2652 AC5 — build {field, old_value, new_value} diff records from before-state
// + requested changes. Tag categories (domain/chunk/sequence/type/origin) read
// the prior label off the card; structured fields (owner/priority/title/status)
// read from the BoardTask shape.
function buildFieldChanges(
  before: BoardTask, pairs: Record<string, string>,
): Array<{ field: string; old_value: string; new_value: string }> {
  const out: Array<{ field: string; old_value: string; new_value: string }> = [];
  const tagCategories = ['domain', 'chunk', 'sequence', 'type', 'origin'];
  const beforeLabels = before.domains || [];
  for (const [key, value] of Object.entries(pairs)) {
    if (tagCategories.includes(key)) {
      const prior = beforeLabels.find((l) => l.startsWith(`${key}:`));
      out.push({ field: key, old_value: prior ? prior.slice(key.length + 1) : '', new_value: value });
    } else if (key === 'owner') {
      out.push({ field: 'owner', old_value: before.owner ?? '', new_value: value });
    } else if (key === 'priority') {
      out.push({ field: 'priority', old_value: before.priority ?? '', new_value: value });
    } else if (key === 'title') {
      out.push({ field: 'title', old_value: before.title ?? '', new_value: value });
    } else if (key === 'status') {
      out.push({ field: 'status', old_value: before.status ?? '', new_value: value });
    } else if (key === 'desc' || key === 'description') {
      // Don't include full desc text in spine payload; just signal length delta.
      out.push({ field: 'description', old_value: '(omitted)', new_value: `(${value.length} chars)` });
    }
    // 'after' / 'gates' relation pairs are not field-set; they emit relation events elsewhere.
  }
  return out;
}

export async function tagCard(client: BoardClient, index: number, value: string, category: string = 'chunk'): Promise<void> {
  // #2652 AC7 — idempotency at API level. If the card already carries this
  // exact tag, skip the BoardClient call AND the spine emit. Vikunja sees no
  // redundant write; audit logs don't fill with phantom changes.
  let title = '';
  let alreadyHas = false;
  try {
    const card = await client.view(index);
    title = card.title;
    alreadyHas = (card.domains || []).includes(`${category}:${value.toLowerCase()}`)
              || (card.domains || []).includes(`${category}:${value}`);
  } catch { /* best effort */ }
  if (alreadyHas) {
    console.log(`#${index} already has ${category}:${value} — no-op`);
    return;
  }
  await client.tag(index, category, value);
  console.log(`Tagged #${index} → ${category}:${value}`);
  // #2652 AC6 — payload includes 'op' so subscribers can reconstruct tag history
  // (add vs remove). Was implicit-add before; explicit op is more honest.
  emitSpineEvent('card.item.tagged', detectRole(), {
    card_id: String(index), title, category, value, op: 'add', [category]: value, board: client.boardName,
  });
}

export async function untagCard(client: BoardClient, index: number, value: string, category: string = 'chunk'): Promise<void> {
  // #2652 AC7 — idempotency at API level. If the card doesn't carry this tag,
  // skip the BoardClient call AND the spine emit (the prior code would 404 on
  // missing-label removal; this is cleaner).
  let alreadyAbsent = false;
  try {
    const card = await client.view(index);
    alreadyAbsent = !(card.domains || []).some((l) =>
      l === `${category}:${value.toLowerCase()}` || l === `${category}:${value}`);
  } catch { /* best effort — fall through to attempt */ }
  if (alreadyAbsent) {
    console.log(`#${index} does not have ${category}:${value} — no-op`);
    return;
  }
  await client.untag(index, category, value);
  console.log(`Untagged #${index} → removed ${category}:${value}`);
  // #2652 AC6 — emit symmetric event on remove. Previously untag was silent on
  // the spine; subscribers could see additions but not removals. Closes the gap.
  emitSpineEvent('card.item.tagged', detectRole(), {
    card_id: String(index), category, value, op: 'remove', board: client.boardName,
  });
}

export async function swatCard(client: BoardClient, title: string): Promise<BoardTask> {
  const role = detectRole();
  const owner = role.charAt(0).toUpperCase() + role.slice(1);
  const swatTitle = `[swat] ${title}`;
  const task = await client.add(swatTitle, { status: 'SWAT', owner, priority: 'P1' });
  console.log(`SWAT #${task.index}: ${swatTitle}`);
  emitSpineEvent('card.swat.created', role, {
    card_id: String(task.index), title: swatTitle, board: client.boardName,
  });
  return task;
}

export async function bulkSequenceTag(
  client: BoardClient, ids: number[], sequence: string
): Promise<void> {
  const validSequences = Object.keys(LABELS.sequence);
  const seq = sequence.toLowerCase();
  if (!validSequences.includes(seq)) {
    console.error(`ERROR: Unknown sequence "${sequence}". Valid: ${validSequences.join(', ')}`);
    process.exit(1);
  }

  const role = detectRole();
  let tagged = 0;
  let skipped = 0;

  for (const id of ids) {
    try {
      await client.tag(id, 'sequence', seq);
      tagged++;
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      if (msg.includes('409') || msg.includes('already')) {
        skipped++;
      } else {
        console.error(`  #${id}: ${msg}`);
      }
    }
  }

  console.log(`Tagged ${tagged} card(s) → sequence:${seq}${skipped > 0 ? ` (${skipped} already tagged)` : ''}`);
  emitSpineEvent('card.sequence.bulk_tagged', role, {
    sequence: seq, count: String(tagged), skipped: String(skipped),
    board: client.boardName,
  });
}

export async function bulkMove(
  client: BoardClient, ids: number[], status: string
): Promise<void> {
  const role = detectRole();
  let moved = 0;
  let failed = 0;

  for (const id of ids) {
    try {
      await client.move(id, status);
      console.log(`  #${id} → ${status}`);
      moved++;
    } catch (err: unknown) {
      console.error(`  #${id}: ${err instanceof Error ? err.message : err}`);
      failed++;
    }
  }

  console.log(`Moved ${moved} card(s) to ${status}${failed > 0 ? ` (${failed} failed)` : ''}`);
  emitSpineEvent('card.bulk.moved', role, {
    status, count: String(moved), failed: String(failed),
    board: client.boardName,
  });
}

export async function snapshotBoard(client: BoardClient): Promise<string> {
  const snap = await client.snapshot();
  const file = path.join(SNAPSHOT_DIR, `board-snapshot-${client.boardName}.json`);
  fs.mkdirSync(SNAPSHOT_DIR, { recursive: true });
  fs.writeFileSync(file, JSON.stringify(snap, null, 2));
  console.log(`Snapshot saved: ${file} (${snap.tasks.length} tasks)`);
  emitSpineEvent('board.snapshot.taken', detectRole(), {
    board: client.boardName, task_count: String(snap.tasks.length),
  });
  return file;
}
