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
      cur.text += '\n' + l.text;
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

function roleState(turns: TurnFact[], now: string): { state: 'moving' | 'quiet'; secondsSinceLastCall: number } {
  const last = turns.reduce((m, t) => (Date.parse(t.lastCallAt) > Date.parse(m) ? t.lastCallAt : m), '1970-01-01T00:00:00Z');
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
  const roles = [...byRole.entries()].map(([role, ts]) => ({ role, ...roleState(ts, input.now) })).sort((a, b) => a.role.localeCompare(b.role));
  const stateOf = new Map(roles.map(r => [r.role, r]));

  const shown = prompts.slice(-(input.lastN ?? LAST_N_PROMPTS)).reverse();
  const nodes: PromptNode[] = shown.map(p => {
    const turns = input.turns.filter(t => promptOf(t.wokenByLine) === p.id);
    const runs = input.runs.filter(r => promptOf(r.startedByLine) === p.id);
    const roleNames = [...new Set([...p.to.filter(r => byRole.has(r)), ...turns.map(t => t.role)])].sort();
    const roleNodes: RoleNode[] = roleNames.map(role => {
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

/** One role's slice: its state and the prompts it is in, nothing of the other roles. */
export function sliceFor(tree: Tree, role: string): { at: string; me?: Tree['roles'][number]; prompts: (Prompt & { mine: RoleNode })[] } {
  return {
    at: tree.at,
    me: tree.roles.find(r => r.role === role),
    prompts: tree.prompts.flatMap(p => {
      const mine = p.roles.find(r => r.role === role);
      return mine ? [{ id: p.id, by: p.by, to: p.to, at: p.at, text: p.text, lines: p.lines, mine }] : [];
    }),
  };
}
