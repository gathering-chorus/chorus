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

export interface PresenceRow { name: string; presenceOf?: string; pane?: string; tty?: string; checkedAt?: string }
export interface RunRow { name: string; runEndedAt?: string; startedAt?: string }

const RUN_PREFIX = 'session-run-';

export function resolveFromPresence(presences: PresenceRow[], runs: RunRow[], role: string): TypedResolution {
  const mine = presences.filter((p) => p.name.startsWith(`${role}-presence-`));
  if (mine.length === 0) return { kind: 'unregistered' };
  const live = new Set(runs.filter((r) => r.name.startsWith(`${role}-run-`) && !r.runEndedAt).map((r) => r.name));
  const current = mine
    .filter((p) => live.has((p.presenceOf ?? '').replace(RUN_PREFIX, '')))
    .sort((a, b) => (b.checkedAt ?? '').localeCompare(a.checkedAt ?? ''))[0];
  if (!current) return { kind: 'dead' };
  const session: SessionReg = {
    role,
    pid: 0,
    tty: current.tty ?? '',
    host: current.pane ? 'tmux' : 'unknown',
    ...(current.pane ? { tmux: current.pane } : {}),
  };
  return { kind: 'resolved', session };
}

/** Read both collections from athena-make. A failed read is its own answer
 * (never a fallback to the registry files): the caller reports it typed. */
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
