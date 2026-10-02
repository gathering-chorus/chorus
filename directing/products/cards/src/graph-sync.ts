/**
 * #3102 — keep each card's graph row current with the board.
 *
 * The board (Vikunja) is the system of record. Every Card row in
 * urn:chorus:domains:cards is a copy of one board card, written through
 * athena-make's generated door:
 *   - after every cards write (syncCardToGraph), in the same call, and
 *   - by the reconcile pass (reconcileGraph), which diffs the whole board
 *     against the graph, repairs the differences and reports how many rows
 *     were out of sync. That number is the measure: it should read 0.
 *
 * A failed graph write never fails the board write. It is reported on stderr
 * and in the spine (card.graph.sync_failed), and the reconcile pass repairs it.
 */
import { execFileSync } from 'child_process';
import * as path from 'path';
import { BoardTask } from './types';
import { emitSpineEvent } from './events';

const ATHENA_MAKE = process.env.ATHENA_MAKE_URL || 'http://localhost:3360';
const CARDS_ROUTE = '/cards/cards';

/** The fields a Card row carries, as the door takes them. */
export interface CardRow {
  label: string;
  status?: string;
  assignee?: string;
  priority?: string;
  cardType?: string;
}

const PRIORITIES = new Set(['P1', 'P2', 'P3']);
const CARD_TYPES = new Set(['new', 'enhance', 'fix', 'chore', 'swat', 'issue']);
const ROLES = new Set(['jeff', 'wren', 'silas', 'kade']);

/** One board card → the row its graph copy should hold. Pure. */
export function cardRow(task: BoardTask): CardRow {
  const row: CardRow = { label: task.title };
  if (task.status) row.status = task.status;
  const owner = (task.owner || '').toLowerCase();
  if (ROLES.has(owner)) row.assignee = owner;
  const pri = (task.priority || '').toUpperCase();
  if (PRIORITIES.has(pri)) row.priority = pri;
  const type = task.domains.find((d) => d.startsWith('type:'))?.slice(5);
  if (type && CARD_TYPES.has(type)) row.cardType = type;
  return row;
}

/** A row as the generated route serves it, reduced to the fields we sync. */
export function servedRow(r: Record<string, unknown>): CardRow {
  const str = (v: unknown) => (typeof v === 'string' && v !== '' ? v : undefined);
  const roleName = (v: unknown) => str(v)?.replace(/^.*[#/]/, '').replace(/^role-/, '');
  const out: CardRow = { label: str(r.label) ?? '' };
  const status = str(r.status);
  const assignee = roleName(r.assignee);
  const priority = str(r.priority);
  const cardType = str(r.cardType);
  if (status) out.status = status;
  if (assignee) out.assignee = assignee;
  if (priority) out.priority = priority;
  if (cardType) out.cardType = cardType;
  return out;
}

/** Field names where the graph row differs from the board. Empty = in sync. */
export function rowDiff(want: CardRow, have: CardRow | undefined): string[] {
  if (!have) return ['missing'];
  const pairs: Array<[string, string | undefined, string | undefined]> = [
    ['label', want.label, have.label],
    ['status', want.status, have.status],
    ['assignee', want.assignee, have.assignee],
    ['priority', want.priority, have.priority],
    ['cardType', want.cardType, have.cardType],
  ];
  return pairs.filter(([, w, h]) => (w ?? '') !== (h ?? '')).map(([name]) => name);
}

/** The role this process writes as. Row ownership follows the writer (the door's rule). */
function writerRole(): string {
  return (process.env.DEPLOY_ROLE || process.env.CHORUS_ROLE || 'wren').toLowerCase();
}

let cachedToken: { role: string; token: string; at: number } | null = null;
function identityToken(role: string): string {
  // Tokens live about 600s; take a fresh one every 5 minutes.
  if (cachedToken && cachedToken.role === role && Date.now() - cachedToken.at < 300_000) return cachedToken.token;
  const script = process.env.CHORUS_IDENTITY_TOKEN_BIN
    || path.join(process.env.CHORUS_ROOT || path.join(process.env.HOME || '', 'CascadeProjects/chorus'), 'platform/scripts/chorus-identity-token');
  const token = execFileSync(script, [role], { encoding: 'utf-8', timeout: 15_000 }).trim();
  if (!token) throw new Error(`no identity token for ${role}`);
  cachedToken = { role, token, at: Date.now() };
  return token;
}

export interface GraphDoor {
  get(index: number): Promise<CardRow | undefined>;
  list(): Promise<Map<number, CardRow>>;
  put(index: number, row: CardRow): Promise<void>;
  /** Create many rows in one write. Rows that already exist are refused, so
   *  only missing cards go through here. */
  createBatch(rows: Array<{ index: number; row: CardRow }>): Promise<void>;
}

/** #3102 — missing cards go in batches; one write per card took ~2s, so a
 *  4,300-card first sync would run for hours. */
export const BATCH_SIZE = 200;

/** The real door: athena-make's generated /cards/cards route. */
export function athenaMakeDoor(fetchImpl: typeof fetch = fetch): GraphDoor {
  const auth = () => ({ Authorization: `Bearer ${identityToken(writerRole())}`, 'Content-Type': 'application/json' });
  return {
    async get(index) {
      const r = await fetchImpl(`${ATHENA_MAKE}${CARDS_ROUTE}/${index}`);
      if (r.status === 404) return undefined;
      if (!r.ok) throw new Error(`athena-make ${r.status} reading card ${index}`);
      const body = await r.json() as { data?: Record<string, unknown> };
      return body.data ? servedRow(body.data) : undefined;
    },
    async list() {
      const r = await fetchImpl(`${ATHENA_MAKE}${CARDS_ROUTE}?limit=100000`);
      if (!r.ok) throw new Error(`athena-make ${r.status} listing cards`);
      const body = await r.json() as { data?: Array<Record<string, unknown>> };
      const out = new Map<number, CardRow>();
      for (const d of body.data || []) {
        const id = Number((typeof d.name === 'string' ? d.name : '').replace(/^card-/, ''));
        if (Number.isFinite(id) && id > 0) out.set(id, servedRow(d));
      }
      return out;
    },
    async put(index, row) {
      const body = JSON.stringify({ ...row });
      let r = await fetchImpl(`${ATHENA_MAKE}${CARDS_ROUTE}/${index}`, { method: 'PUT', headers: auth(), body });
      if (r.status === 404) {
        r = await fetchImpl(`${ATHENA_MAKE}${CARDS_ROUTE}`, { method: 'POST', headers: auth(), body: JSON.stringify({ name: String(index), ...row }) });
      }
      if (!r.ok) throw new Error(`athena-make ${r.status} writing card ${index}: ${(await r.text()).slice(0, 200)}`);
    },
    async createBatch(rows) {
      const body = JSON.stringify(rows.map(({ index, row }) => ({ name: String(index), ...row })));
      const r = await fetchImpl(`${ATHENA_MAKE}${CARDS_ROUTE}/batch`, { method: 'POST', headers: auth(), body });
      if (!r.ok) throw new Error(`athena-make ${r.status} creating ${rows.length} cards: ${(await r.text()).slice(0, 200)}`);
    },
  };
}

/**
 * After a board write: copy that card to the graph. Never throws — the board
 * change already happened; a failure is reported and the reconcile pass repairs it.
 */
export async function syncCardToGraph(
  task: BoardTask,
  door: GraphDoor = athenaMakeDoor(),
): Promise<boolean> {
  if (process.env.CARDS_GRAPH_SYNC === 'off') return false;
  try {
    await door.put(task.index, cardRow(task));
    return true;
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    process.stderr.write(`WARN: card #${task.index} changed on the board but its graph row was not updated: ${message}\n  The reconcile pass (cards graph-sync) will repair it.\n`);
    emitSpineEvent('card.graph.sync_failed', writerRole(), { card: String(task.index), reason: message.slice(0, 160) });
    return false;
  }
}

export interface ReconcileResult {
  boardCards: number;
  outOfSync: number;
  repaired: number;
  failed: Array<{ index: number; reason: string }>;
  sample: Array<{ index: number; fields: string[] }>;
  /** Card numbers the board gives to more than one card. One number can be one
   *  graph row only, so these are reported and left for a person to fix. */
  duplicateNumbers: number[];
}

type Pending = Array<{ index: number; row: CardRow }>;

/** Card numbers the board gives to more than one card, sorted. */
function duplicateNumbers(board: BoardTask[]): number[] {
  const seen = new Map<number, number>();
  for (const t of board) seen.set(t.index, (seen.get(t.index) ?? 0) + 1);
  return [...seen].filter(([, n]) => n > 1).map(([i]) => i).sort((a, b) => a - b);
}

/** Compare each board card with its row; fill the counts and split the work. */
function classify(board: BoardTask[], graph: Map<number, CardRow>, res: ReconcileResult): { missing: Pending; stale: Pending } {
  const dup = new Set(res.duplicateNumbers);
  const missing: Pending = [];
  const stale: Pending = [];
  for (const task of board.filter((t) => !dup.has(t.index))) {
    const want = cardRow(task);
    const fields = rowDiff(want, graph.get(task.index));
    if (fields.length === 0) continue;
    res.outOfSync++;
    if (res.sample.length < 10) res.sample.push({ index: task.index, fields });
    (fields[0] === 'missing' ? missing : stale).push({ index: task.index, row: want });
  }
  return { missing, stale };
}

function reason(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/** Write the missing rows in batches and the stale ones one by one. */
async function applyRepairs(door: GraphDoor, missing: Pending, stale: Pending, res: ReconcileResult): Promise<void> {
  for (let i = 0; i < missing.length; i += BATCH_SIZE) {
    const chunk = missing.slice(i, i + BATCH_SIZE);
    try {
      await door.createBatch(chunk);
      res.repaired += chunk.length;
    } catch (err) {
      for (const c of chunk) res.failed.push({ index: c.index, reason: reason(err) });
    }
  }
  for (const s of stale) {
    try {
      await door.put(s.index, s.row);
      res.repaired++;
    } catch (err) {
      res.failed.push({ index: s.index, reason: reason(err) });
    }
  }
}

/** Diff the whole board against the graph; repair unless dryRun. */
export async function reconcileGraph(
  board: BoardTask[],
  door: GraphDoor,
  opts: { dryRun?: boolean } = {},
): Promise<ReconcileResult> {
  const graph = await door.list();
  const res: ReconcileResult = {
    boardCards: board.length, outOfSync: 0, repaired: 0, failed: [], sample: [], duplicateNumbers: duplicateNumbers(board),
  };
  const { missing, stale } = classify(board, graph, res);
  if (!opts.dryRun) await applyRepairs(door, missing, stale, res);
  return res;
}
