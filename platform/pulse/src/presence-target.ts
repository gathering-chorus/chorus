/**
 * #4361 — where is a role? Its Presence row says.
 *
 * One answer, from the model: chorus-principal writes a Presence (pane, tty)
 * for each SessionRun it starts, and ends the run on logout or exit. A role is
 * reachable at the Presence of its live run (runEndedAt empty). The registry
 * files under ~/.chorus/sessions are no longer read for routing.
 *
 * The result reuses TypedResolution so transport routing (tmux / tty) stays in
 * planDelivery, unchanged.
 */
import type { SessionReg, TypedResolution } from './session-registry';

export interface PresenceRow { name: string; presenceOf?: string; pane?: string; tty?: string; checkedAt?: string; reachableOver?: string }
export interface RunRow { name: string; runEndedAt?: string; startedAt?: string }

/** presenceOf names the run as the API stored it: "<kind>-<run name>"
 * ("session-run-wren-run-x" live). Match on the run name, whatever the prefix. */
function ofLiveRun(presenceOf: string, live: Set<string>): boolean {
  return liveRunOf(presenceOf, live) !== null;
}
function liveRunOf(presenceOf: string, live: Set<string>): string | null {
  for (const run of live) if (presenceOf === run || presenceOf.endsWith(`-${run}`)) return run;
  return null;
}
/** #4424 — reachableOver names a Channel; the store may keep the bare kind
 * ("agent") or the row name ("channel-agent"). */
function overAgent(p: PresenceRow): boolean {
  const over = p.reachableOver ?? '';
  return over === 'agent' || over.endsWith('-agent');
}

export function resolveFromPresence(presences: PresenceRow[], runs: RunRow[], role: string): TypedResolution {
  const mine = presences.filter((p) => p.name.startsWith(`${role}-presence-`));
  if (mine.length === 0) return { kind: 'unregistered' };
  const live = new Set(runs.filter((r) => r.name.startsWith(`${role}-run-`) && !r.runEndedAt).map((r) => r.name));
  // #4362 — only a tmux pane id (%N) is a pane. A run logged in outside tmux
  // stores "-", which tmux reads as "the current pane": every nudge typed into
  // whoever was on screen. A live run with a real pane wins; then the newest.
  const paneOf = (p: PresenceRow): string => (/^%\d+$/.test(p.pane ?? '') ? (p.pane as string) : '');
  const candidates = mine
    .filter((p) => ofLiveRun(p.presenceOf ?? '', live))
    .sort((a, b) => Number(!!paneOf(b)) - Number(!!paneOf(a)) || (b.checkedAt ?? '').localeCompare(a.checkedAt ?? ''));
  if (candidates.length === 0) return { kind: 'dead' };
  // #4424 — a run on another model is reached through the agent supervisor,
  // addressed by the run's own name (the supervisor's session id).
  const agent = candidates.find(overAgent);
  if (agent) {
    const run = liveRunOf(agent.presenceOf ?? '', live);
    if (run) return { kind: 'agent', run };
  }
  const current = candidates[0];
  const pane = paneOf(current);
  // No live run has a pane: typing by tty would land in whatever that terminal
  // runs now (on 2026-10-01, silas's only live run was a werk-demo on ttys007).
  if (!pane) return { kind: 'no-pane' };
  const session: SessionReg = { role, pid: 0, tty: current.tty ?? '', host: 'tmux', tmux: pane };
  return { kind: 'resolved', session };
}

/** Read both collections from athena-make. A failed read is its own answer
 * (never a fallback to the registry files): the caller reports it typed. */
/** #4361 — both ends of a delivery: the target's resolution, and the
 * sender's session (for the #3352 same-session rule). */
export async function resolveEnds(to: string, from?: string): Promise<{ toRes: TypedResolution | { kind: 'unread'; why: string }; sender: SessionReg | null }> {
  const toRes = await fetchPresenceResolution(to);
  const fromRes = from ? await fetchPresenceResolution(from) : null;
  return { toRes, sender: fromRes?.kind === 'resolved' ? fromRes.session : null };
}

export async function fetchPresenceResolution(role: string, base = process.env.ATHENA_MAKE_URL || 'http://localhost:3360'): Promise<TypedResolution | { kind: 'unread'; why: string }> {
  try {
    const get = async (p: string): Promise<unknown[]> => {
      const r = await fetch(`${base}/v1/identity/${p}?limit=5000`, { signal: AbortSignal.timeout(2000) });
      if (!r.ok) throw new Error(`${p} answered HTTP ${r.status}`);
      const body = (await r.json()) as { data?: unknown[] };
      return body.data ?? [];
    };
    const [presences, runs] = await Promise.all([get('presences'), get('sessionruns')]);
    return resolveFromPresence(presences as PresenceRow[], runs as RunRow[], role);
  } catch (e) {
    return { kind: 'unread', why: e instanceof Error ? e.message : String(e) };
  }
}

/** #4362 — a role's live run has no pane (it was logged in outside tmux), but
 * the role itself runs in its own tmux session, chorus-<role>. Read that
 * session's panes: exactly one %N pane is the role's pane. Anything else
 * (no session, several panes, not a pane id) reaches no one. */
export function paneFromTmuxListing(listing: string): string {
  const panes = listing.split('\n').map((l) => l.trim()).filter(Boolean);
  return panes.length === 1 && /^%\d+$/.test(panes[0]) ? panes[0] : '';
}

export function rolePaneFromTmux(role: string, run: (args: string[]) => string): string {
  try {
    return paneFromTmuxListing(run(['list-panes', '-t', `chorus-${role}`, '-F', '#{pane_id}']));
  } catch {
    return '';
  }
}
