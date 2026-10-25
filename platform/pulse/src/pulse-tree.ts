/**
 * pulse-tree.ts — #4638: the live tree, derived once in pulse.
 *
 * Jeff 2026-10-19: prompt → role (moving/quiet, where) → turn (thoughts,
 * streams) and card (pipeline, verbs, notes). "if we do that then all of u
 * have operational awareness in a tree". The nouns are Prompt, Turn and Note
 * in roles/wren/ontology/pulse-tree-4638.ttl; this is the pure derivation
 * over them. Clearing only draws the result, and a role's turn-start context
 * is its slice of it.
 *
 * A burst of lines from one actor within BURST_GAP_S of each other is ONE
 * prompt (Jeff 2026-10-23 14:56, 15:00): roles answer the joined prompt,
 * never a fragment. A sender that is not a real actor never makes a prompt;
 * it is counted, never stored as unknown (#4637).
 */

export const BURST_GAP_S = 5;
export const QUIET_AFTER_S = 120;
export const LAST_N_PROMPTS = 5;

export interface LineFact { id: string; from: string; to: string[]; text: string; sentAt: string }
export interface TurnFact {
  id: string; role: string; run: string; startedAt: string; endedAt?: string; lastCallAt: string;
  /** the line that woke the turn; absent when a nudge or a pipeline did */
  wokenByLine?: string;
  card?: number;
}
export interface RunFact { id: string; card: number; pipeline: string; step: string; status: string; startedByLine?: string }
export interface NoteFact { id: string; turn: string; card?: number; text: string; at: string }

export interface TreeInput {
  now: string;
  actors: string[];
  lines: LineFact[];
  turns: TurnFact[];
  runs: RunFact[];
  notes: NoteFact[];
  /** each role's last tool call, from the actor-state stream, until turns carry their own */
  lastCalls?: Record<string, string>;
  lastN?: number;
}

export interface Prompt { id: string; by: string; to: string[]; at: string; text: string; lines: string[] }
export interface CardNode { card: number; runs: RunFact[]; notes: NoteFact[] }
export interface TurnNode { id: string; startedAt: string; endedAt?: string; card?: number; notes: NoteFact[] }
export interface RoleNode { role: string; state: 'moving' | 'quiet'; secondsSinceLastCall: number; turns: TurnNode[]; cards: CardNode[] }
export interface PromptNode extends Prompt { roles: RoleNode[]; runs: RunFact[] }
export interface Tree { at: string; prompts: PromptNode[]; roles: Pick<RoleNode, 'role' | 'state' | 'secondsSinceLastCall'>[]; refusedSenders: Record<string, number> }

const secs = (a: string, b: string) => Math.round((Date.parse(a) - Date.parse(b)) / 1000);

/** Join each actor's lines into prompts; lines from non-actors are counted, not kept. */
export function joinPrompts(lines: LineFact[], actors: string[]): { prompts: Prompt[]; lineToPrompt: Map<string, string>; refused: Record<string, number> } {
  const real = new Set(actors);
  const refused: Record<string, number> = {};
  const prompts: Prompt[] = [];
  const lineToPrompt = new Map<string, string>();
  const open = new Map<string, Prompt>();
  const lastAt = new Map<string, string>();
  for (const l of [...lines].sort((a, b) => Date.parse(a.sentAt) - Date.parse(b.sentAt))) {
    if (!real.has(l.from)) { refused[l.from || '(none)'] = (refused[l.from || '(none)'] ?? 0) + 1; continue; }
    const cur = open.get(l.from);
    const prev = lastAt.get(l.from);
    if (cur && prev && secs(l.sentAt, prev) <= BURST_GAP_S) {
      // one line sent to two roles is stored once per role: same words, one line of the prompt
      if (cur.text.split('\n').pop() !== l.text) cur.text += '\n' + l.text;
      cur.lines.push(l.id);
      for (const t of l.to) if (!cur.to.includes(t)) cur.to.push(t);
    } else {
      const p: Prompt = { id: `prompt-${l.id}`, by: l.from, to: [...l.to], at: l.sentAt, text: l.text, lines: [l.id] };
      prompts.push(p);
      open.set(l.from, p);
    }
    lastAt.set(l.from, l.sentAt);
    lineToPrompt.set(l.id, open.get(l.from)!.id);
  }
  return { prompts, lineToPrompt, refused };
}

function roleState(calls: string[], now: string): { state: 'moving' | 'quiet'; secondsSinceLastCall: number } {
  const last = calls.reduce((m, c) => (Date.parse(c) > Date.parse(m) ? c : m), '1970-01-01T00:00:00Z');
  const s = secs(now, last);
  return { state: s < QUIET_AFTER_S ? 'moving' : 'quiet', secondsSinceLastCall: s };
}

export function buildTree(input: TreeInput): Tree {
  const { prompts, lineToPrompt, refused } = joinPrompts(input.lines, input.actors);
  const promptOf = (line?: string) => (line ? lineToPrompt.get(line) : undefined);
  const notesByTurn = new Map<string, NoteFact[]>();
  for (const n of input.notes) notesByTurn.set(n.turn, [...(notesByTurn.get(n.turn) ?? []), n]);

  const byRole = new Map<string, TurnFact[]>();
  for (const t of input.turns) byRole.set(t.role, [...(byRole.get(t.role) ?? []), t]);
  const lastCalls = input.lastCalls ?? {};
  const roleNames = [...new Set([...byRole.keys(), ...Object.keys(lastCalls)])];
  const roles = roleNames
    .map(role => ({ role, ...roleState([...(byRole.get(role) ?? []).map(t => t.lastCallAt), ...(lastCalls[role] ? [lastCalls[role]] : [])], input.now) }))
    .sort((a, b) => a.role.localeCompare(b.role));
  const stateOf = new Map(roles.map(r => [r.role, r]));

  const shown = prompts.slice(-(input.lastN ?? LAST_N_PROMPTS)).reverse();
  const nodes: PromptNode[] = shown.map(p => {
    const turns = input.turns.filter(t => promptOf(t.wokenByLine) === p.id);
    const runs = input.runs.filter(r => promptOf(r.startedByLine) === p.id);
    const named = [...new Set([...p.to.filter(r => stateOf.has(r)), ...turns.map(t => t.role)])].sort();
    const roleNodes: RoleNode[] = named.map(role => {
      const mine = turns.filter(t => t.role === role).sort((a, b) => Date.parse(b.startedAt) - Date.parse(a.startedAt));
      const cards = [...new Set(mine.map(t => t.card).filter((c): c is number => c !== undefined))].map(card => ({
        card,
        runs: runs.filter(r => r.card === card),
        notes: mine.flatMap(t => (notesByTurn.get(t.id) ?? []).filter(n => n.card === card)),
      }));
      const s = stateOf.get(role) ?? { state: 'quiet' as const, secondsSinceLastCall: -1 };
      return {
        role, state: s.state, secondsSinceLastCall: s.secondsSinceLastCall, cards,
        turns: mine.map(t => ({ id: t.id, startedAt: t.startedAt, endedAt: t.endedAt, card: t.card, notes: (notesByTurn.get(t.id) ?? []).filter(n => n.card === undefined) })),
      };
    });
    return { ...p, roles: roleNodes, runs };
  });
  return { at: input.now, prompts: nodes, roles, refusedSenders: refused };
}

/** One role's slice: its state, every role's moving/quiet (one line each), and only the prompts it is in. */
export function sliceFor(tree: Tree, role: string): { at: string; me?: Tree['roles'][number]; team: Tree['roles']; prompts: (Prompt & { mine: RoleNode })[] } {
  return {
    at: tree.at,
    me: tree.roles.find(r => r.role === role),
    team: tree.roles,
    prompts: tree.prompts.flatMap(p => {
      const mine = p.roles.find(r => r.role === role);
      return mine ? [{ id: p.id, by: p.by, to: p.to, at: p.at, text: p.text, lines: p.lines, mine }] : [];
    }),
  };
}

// --- live wiring ---------------------------------------------------------------

type ActorStates = { derivedAt?: string; actors?: Array<{ aboutPrincipal?: string; stream?: Array<{ timestamp?: string }> }> };
/** store.queryMessages types its rows as Message (createdAt) but returns raw rows (created_at): read either. */
type StoredLine = { id: number; from: string; to: string; content: string; created_at?: string; createdAt?: string };
type LineStore = { queryMessages: (o: { type?: string; since?: string; limit?: number }) => StoredLine[] };

const role = (principal = '') => principal.replace(/^principal-/, '');
/** pulse's store writes UTC without a zone ("2026-10-24 20:51:53") */
const utc = (t: string) => (/[zZ]|[+-]\d\d:?\d\d$/.test(t) ? new Date(t).toISOString() : new Date(t.replace(' ', 'T') + 'Z').toISOString());

/** The actors are whoever the roles door lists (via actor-state); a role's last call is its newest stream action. */
export function factsFromActorState(states: unknown): { actors: string[]; lastCalls: Record<string, string> } {
  const actors: string[] = [];
  const lastCalls: Record<string, string> = {};
  for (const a of (states as ActorStates)?.actors ?? []) {
    const r = role(a.aboutPrincipal);
    if (!r) continue;
    actors.push(r);
    const ts = (a.stream ?? []).map(x => x.timestamp).filter((x): x is string => !!x).map(utc);
    if (ts.length) lastCalls[r] = ts.reduce((m, c) => (Date.parse(c) > Date.parse(m) ? c : m));
  }
  return { actors, lastCalls };
}

export function liveTree(store: LineStore, states: unknown, now = new Date().toISOString(), lastN = LAST_N_PROMPTS, spine: SpineLine[] = []): Tree {
  const since = new Date(Date.parse(now) - 6 * 3600_000).toISOString().replace('T', ' ').slice(0, 19);
  const lines: LineFact[] = store.queryMessages({ type: 'jeff-input', since, limit: 500 }).map(m => ({
    id: String(m.id), from: m.from, to: m.to ? [m.to] : [], text: m.content, sentAt: utc(m.created_at ?? m.createdAt ?? ''),
  }));
  const { actors, lastCalls } = factsFromActorState(states);
  const { turns, runs } = factsFromSpine(spine, lastCalls);
  return buildTree({ now, actors, lines, turns, runs, notes: [], lastCalls, lastN });
}

/** GET /api/tree (whole tree), GET /api/tree/:role (one role's slice). Read-only. */
export function registerTreeRoutes(
  app: { get: (path: string, h: (req: { params: Record<string, string>; query: Record<string, unknown> }, res: { json: (b: unknown) => void }) => void) => void },
  store: LineStore,
  actorStateService?: { getActorStates: () => unknown },
  spine?: { get: () => SpineLine[] },
): void {
  const n = (q: unknown) => Math.min(Math.max(parseInt(String(q ?? LAST_N_PROMPTS), 10) || LAST_N_PROMPTS, 1), 50);
  const build = (q: unknown) => liveTree(store, actorStateService?.getActorStates(), new Date().toISOString(), n(q), spine?.get() ?? []);
  app.get('/api/tree', (req, res) => res.json(build(req.query.n)));
  app.get('/api/tree/:role', (req, res) => res.json(sliceFor(build(req.query.n), req.params.role)));
}

// --- turns and runs from the spine (#4637 AC4) -------------------------------

/** One spine line as chorus-log writes it: event, role, timestamp, then its key=value fields. */
export interface SpineLine { event?: string; role?: string; timestamp?: string; [k: string]: unknown }

/**
 * Turns from agent.turn.started (woken_by_line when a Jeff line woke it) closed by the
 * role's next agent.turn.ended; runs from pipeline.run.started (started_by_line). A
 * turn's last call is its end, or for a role's open turn the role's newest action.
 */
export function factsFromSpine(lines: SpineLine[], lastCalls: Record<string, string> = {}): { turns: TurnFact[]; runs: RunFact[] } {
  const ts = (l: SpineLine) => utc(String(l.timestamp ?? ''));
  const sorted = lines.filter(l => l.timestamp && l.role).sort((a, b) => Date.parse(ts(a)) - Date.parse(ts(b)));
  const turns: TurnFact[] = [];
  const open = new Map<string, TurnFact>();
  const runs: RunFact[] = [];
  for (const l of sorted) {
    const r = String(l.role);
    if (l.event === 'agent.turn.started') {
      const t: TurnFact = {
        id: String(l.turn_id ?? `${r}-${ts(l)}`), role: r, run: String(l.session_id ?? ''),
        startedAt: ts(l), lastCallAt: ts(l),
        wokenByLine: l.woken_by === 'line' && l.woken_by_line ? String(l.woken_by_line) : undefined,
      };
      const prev = open.get(r);
      if (prev && !prev.endedAt) prev.endedAt = prev.lastCallAt = t.startedAt;
      turns.push(t);
      open.set(r, t);
    } else if (l.event === 'agent.turn.ended') {
      const t = open.get(r);
      if (t && !t.endedAt) { t.endedAt = t.lastCallAt = ts(l); }
    } else if (l.event === 'pipeline.run.started') {
      runs.push({
        id: String(l.run_id ?? 'unknown'), card: Number(l.card), pipeline: String(l.pipeline ?? ''),
        step: 'started', status: 'started',
        startedByLine: l.started_by === 'line' && l.started_by_line ? String(l.started_by_line) : undefined,
      });
      const t = open.get(r);
      if (t && !t.endedAt && t.card === undefined && Number.isFinite(Number(l.card))) t.card = Number(l.card);
    }
  }
  for (const [r, t] of open) if (!t.endedAt && lastCalls[r] && Date.parse(lastCalls[r]) > Date.parse(t.lastCallAt)) t.lastCallAt = lastCalls[r];
  return { turns, runs };
}

export const SPINE_TREE_QUERY = '{job="platform-chorus"} |~ `"event":"(agent\\.turn\\.(started|ended)|pipeline\\.run\\.started)"`';

/** Polls Loki every 30 s for the tree's spine lines; a failed read keeps the last lines and says why. */
export class SpineTreeFacts {
  private lines: SpineLine[] = [];
  unmeasured?: string;
  private timer?: NodeJS.Timeout;
  constructor(private lokiBase = process.env.LOKI_URL || 'http://localhost:3102') {}
  get(): SpineLine[] { return this.lines; }
  async refresh(): Promise<void> {
    const end = Date.now() * 1e6, start = end - 6 * 3600 * 1e9;
    const url = `${this.lokiBase}/loki/api/v1/query_range?query=${encodeURIComponent(SPINE_TREE_QUERY)}&start=${start}&end=${end}&limit=2000&direction=forward`;
    try {
      const res = await fetch(url, { signal: AbortSignal.timeout(4000) });
      if (!res.ok) { this.unmeasured = `loki HTTP ${res.status}`; return; }
      const body = (await res.json()) as { data?: { result?: Array<{ values?: Array<[string, string]> }> } };
      const out: SpineLine[] = [];
      for (const s of body.data?.result ?? []) for (const [, line] of s.values ?? []) { try { out.push(JSON.parse(line)); } catch { /* not a spine line */ } }
      this.lines = out;
      this.unmeasured = undefined;
    } catch (e: unknown) {
      this.unmeasured = `loki: ${e instanceof Error ? e.message : String(e)}`;
    }
  }
  start(): void { if (this.timer) return; void this.refresh(); this.timer = setInterval(() => void this.refresh(), 30_000); this.timer.unref(); }
}
