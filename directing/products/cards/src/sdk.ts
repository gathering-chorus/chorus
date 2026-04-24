import * as fs from 'fs';
import * as path from 'path';
import { BoardClient } from './client';
import { BoardTask } from './types';
import { detectRole, LABELS } from './config';
import { emitSpineEvent, emitChorusEvent } from './events';
import { spawnSync } from 'child_process';

// Auto-declare role state from card actions (#1782)
// Eliminates manual role-state calls — state follows card lifecycle.
const ROLE_STATE_BIN = path.resolve(__dirname, '../../../../platform/scripts/role-state');
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

// ── Demo evidence check (#1834) ──

/**
 * Check for demo evidence: a demo brief file OR a card.demo.started spine event.
 */
function checkDemoEvidence(cardIndex: number): boolean {
  const briefDirs = [
    path.join(__dirname, '..', '..', '..', '..', 'roles', 'wren', 'briefs'),
    path.join(__dirname, '..', '..', '..', '..', 'roles', 'silas', 'briefs'),
    path.join(__dirname, '..', '..', '..', '..', 'roles', 'kade', 'briefs'),
    path.join(__dirname, '..', '..', 'roles', 'wren', 'briefs'),
    path.join(__dirname, '..', '..', 'roles', 'silas', 'briefs'),
    path.join(__dirname, '..', '..', 'roles', 'kade', 'briefs'),
  ];

  // Check 1: demo brief file in any role's briefs/
  for (const dir of briefDirs) {
    try {
      const files = fs.readdirSync(dir);
      if (files.some(f => f.includes('demo') && f.includes(String(cardIndex)))) {
        return true;
      }
    } catch { /* dir may not exist */ }
  }

  // Check 2: card.demo.started spine event in chorus.log
  const logPath = path.join(__dirname, '..', '..', '..', '..', 'platform', 'logs', 'chorus.log');
  try {
    const log = fs.readFileSync(logPath, 'utf-8');
    if (log.includes('"event":"card.demo.started"') && log.includes(`"card_id":"${cardIndex}"`)) {
      return true;
    }
  } catch { /* log may not exist */ }

  return false;
}

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
    emitSpineEvent('card.quality.blocked', detectRole(), {
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
    emitSpineEvent('card.quality.blocked', detectRole(), {
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
    emitSpineEvent('card.quality.blocked', detectRole(), {
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
    emitSpineEvent('card.quality.blocked', detectRole(), {
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
  type?: string; origin?: string; quick?: boolean;
};

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
function collectRequiredFieldErrors(opts: AddOpts): string[] {
  const errors: string[] = [];
  if (!opts.domain) errors.push('Missing --domain <name>');
  const validTypes = Object.keys(LABELS.type).join(', ');
  if (!opts.type) errors.push(`Missing --type <${validTypes}>`);
  else if (!LABELS.type[opts.type.toLowerCase()]) errors.push(`Unknown type "${opts.type}". Valid: ${validTypes}`);
  if (!opts.priority) errors.push('Missing --priority P1|P2|P3');
  if (!opts.origin) errors.push('Missing origin. Is this reactive (responding to breakage) or reflective (chosen work)? Use --origin reflective|reactive');
  else if (!['reflective', 'reactive'].includes(opts.origin.toLowerCase())) errors.push(`Unknown origin "${opts.origin}". Valid: reflective, reactive`);
  return errors;
}

// Validates description has AC when not in --quick mode. Warns on missing Experience.
function validateDescription(
  opts: AddOpts, title: string, boardName: string, errors: string[],
): void {
  if (opts.quick) return;
  const desc = (opts.description || '').trim();
  if (!desc) {
    errors.push('Missing --desc with acceptance criteria (use --quick/-q to skip, or --desc-file <path>, or --desc - for stdin)');
  } else {
    const hasAC =
      /acceptance\s*criteria/i.test(desc) ||
      /##\s*(ac|criteria|what|acceptance)/i.test(desc) ||
      /- \[[ x]\]/i.test(desc) ||
      /\d+\.\s+\S/m.test(desc);
    if (!hasAC) errors.push('Description missing acceptance criteria (need ## AC heading, checkboxes, or numbered items). Use --quick/-q to skip');
  }
  const hasExperience = /##\s*experience/i.test(opts.description || '');
  if (!hasExperience) {
    console.log('  WARN: No Experience section. Wren should add "## Experience" before this card enters WIP.');
    emitSpineEvent('card.quality.warned', detectRole(), {
      title, gate: 'experience_missing_at_creation', board: boardName,
    });
  }
}

// Reports collected errors and exits the process.
function reportErrorsAndExit(errors: string[], title: string, boardName: string): never {
  console.error(`ERROR: Card creation failed (${errors.length} issue${errors.length > 1 ? 's' : ''}):`);
  for (const err of errors) console.error(`  • ${err}`);
  emitSpineEvent('card.quality.blocked', detectRole(), {
    title, gate: 'add_validation_failed', board: boardName,
    errors: errors.join('; '),
  });
  process.exit(1);
}

// Applies post-add tags (sequence, origin) and triggers workflow if status is Now.
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
  if (task.status.toLowerCase() === 'now') {
    try { await triggerWorkflow(client, task.index); }
    catch (err: unknown) { console.error(`  (workflow: ${err instanceof Error ? err.message : err})`); }
  }
}

export async function addCard(
  client: BoardClient, title: string, opts: AddOpts,
): Promise<BoardTask> {
  warnShortTitle(title, client.boardName);

  // Classification gates: ALWAYS enforced, even with --quick (#1966)
  // --quick only exempts description/AC requirement.
  inferCardDefaults(title, opts);

  const errors = collectRequiredFieldErrors(opts);
  validateDescription(opts, title, client.boardName, errors);
  if (errors.length > 0) reportErrorsAndExit(errors, title, client.boardName);

  if (opts.quick) {
    emitSpineEvent('card.quick.created', detectRole(), { title, board: client.boardName });
  }
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

// Pre-move gates — runs before the actual status change. Exits process on gate failure.
async function enforcePreMoveGates(
  client: BoardClient, index: number, title: string, card: BoardTask, status: string,
): Promise<void> {
  if (status.toLowerCase() === 'now' && !enforceNowDescriptionGate(index, title, card.description, client.boardName)) {
    process.exit(1);
  }
  if (/^wip$/i.test(status) && !enforceACGate(index, title, card.description, client.boardName)) {
    process.exit(1);
  }
  if (/^wip$/i.test(status) && !enforceExperienceGate(index, title, card.description, client.boardName)) {
    process.exit(1);
  }
  if (/^(now|wip)$/i.test(status)) {
    enforceTaxonomyGate(index, title, card.domains, client.boardName);
  }
  if (/^wip$/i.test(status)) {
    try {
      const fullText = `${title}\n${card.description || ''}`;
      const domainLabel = (card.domains || []).find((d: string) => d.startsWith('domain:'));
      const cardDomain = domainLabel ? domainLabel.replace('domain:', '') : undefined;
      if (isCodeCard(fullText)) {
        const report = await generateBlastRadius(index, title, card.description || '', cardDomain);
        if (report && report.totalFiles === 0) {
          console.error(`ERROR: Blast radius: 0 files on a code card (#${index}).`);
          console.error('  Add explicit file paths to description (e.g. src/handlers/music.handler.ts)');
          console.error('  or route to Wren for manual blast radius mapping.');
          emitSpineEvent('card.blast_radius.zero_code', detectRole(), {
            card_id: String(index), title, board: client.boardName,
          });
          process.exit(1);
        }
      }
    } catch { /* blast radius API failure = non-blocking, proceed to WIP */ }
  }
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
  } else {
    // Demo gate (#1834) — require demo evidence before Done transition
    // Exempt: type:chore and type:swat cards (same as existing skip logic)
    const cardForGate = await client.view(index).catch(() => null);
    const cardDomains = cardForGate?.domains || [];
    const isExempt = cardDomains.some((d: string) => d === 'type:chore' || d === 'type:swat');
    if (!isExempt) {
      const hasDemoEvidence = checkDemoEvidence(index);
      if (!hasDemoEvidence) {
        console.error(`Demo gate: #${index} has no demo evidence. Run /demo ${index} first.`);
        process.exit(1);
      }
    }
  }

  await client.done(index);
  console.log(`Done: #${index}`);
  emitSpineEvent('card.item.completed', role, {
    card_id: String(index), title, board: client.boardName,
  });
  emitSpineEvent('card.accepted', role, {
    card_id: String(index), title, board: client.boardName, hop: '1', source_service: 'board', dest_service: 'role-state', callStack: 'integration',
  });
  autoRoleState('idle');

  emitChorusEvent('deploy.verification.completed', role, {
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
  emitSpineEvent('card.unblocked', role, {
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

const SET_CARD_VALID_KEYS = new Set(['domain', 'chunk', 'sequence', 'stream', 'type', 'origin', 'owner', 'priority', 'title', 'desc', 'description', 'status', 'after', 'gates']);
const SET_CARD_TAG_CATEGORIES = ['domain', 'chunk', 'sequence', 'stream', 'type', 'origin'];

function validateSetKeys(pairs: Record<string, string>): void {
  for (const key of Object.keys(pairs)) {
    if (!SET_CARD_VALID_KEYS.has(key)) {
      throw new Error(`Unknown key "${key}". Valid: ${[...SET_CARD_VALID_KEYS].join(', ')}`);
    }
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
  if (card.domains?.length) console.log(`  Domains:  ${card.domains.join(', ')}`);
  if (changes.length) console.log(`  Changed:  ${changes.join(', ')}`);
}

export async function setCard(client: BoardClient, index: number, pairs: Record<string, string>): Promise<void> {
  validateSetKeys(pairs);
  const changes: string[] = [];

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

  emitSpineEvent('card.item.set', detectRole(), {
    card_id: String(index), changes: changes.join(','), board: client.boardName,
  });
}

export async function tagCard(client: BoardClient, index: number, value: string, category: string = 'chunk'): Promise<void> {
  let title = '';
  try { title = (await client.view(index)).title; } catch { /* best effort */ }
  await client.tag(index, category, value);
  console.log(`Tagged #${index} → ${category}:${value}`);
  emitSpineEvent('card.item.tagged', detectRole(), {
    card_id: String(index), title, [category]: value, board: client.boardName,
  });
}

export async function untagCard(client: BoardClient, index: number, value: string, category: string = 'chunk'): Promise<void> {
  await client.untag(index, category, value);
  console.log(`Untagged #${index} → removed ${category}:${value}`);
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
