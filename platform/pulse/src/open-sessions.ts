/**
 * #4948 — pulse's one read of the open sessions, shared by actor-state and
 * changes-overnight (each kept its own copy of the #4838 query). Two door queries:
 * - sign-ins: no endedAt, expiresAt within the last hour or later (#4838)
 * - agent (pane) sessions: no endedAt, no expiry filter. An agent's session lasts
 *   from login to logout; its expiresAt is written once at login and never renewed,
 *   so the first query would drop it an hour after login (Silas, pair 4948).
 * Either query failing is an Error (the caller marks it unmeasured), never "nobody".
 */
export interface RawSessionRow {
  name: string;
  ownedBy?: string;
  channel?: string;
  expiresAt?: string;
  lastSeenAt?: string;
  endedAt?: string;
  sessionState?: string;
}

export function liveSessionsQuery(nowMs: number = Date.now()): string {
  const after = new Date(nowMs - 60 * 60 * 1000).toISOString().replace(/\.\d{3}Z$/, 'Z');
  return `endedAt.absent=true&expiresAt.after=${encodeURIComponent(after)}&limit=5000`;
}

export function agentSessionsQuery(): string {
  return 'endedAt.absent=true&channel=pane&limit=5000';
}

export async function fetchOpenSessionRows(athenaBase: string, timeoutMs: number, nowMs: number = Date.now()): Promise<RawSessionRow[]> {
  const queries = [liveSessionsQuery(nowMs), agentSessionsQuery()];
  const bodies = await Promise.all(queries.map(async (q) => {
    const res = await fetch(`${athenaBase}/v1/identity/sessions?${q}`, { signal: AbortSignal.timeout(timeoutMs) });
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const json = (await res.json()) as { data?: unknown };
    if (!Array.isArray(json.data)) throw new Error('the sessions door answered with no data list');
    return json.data as RawSessionRow[];
  }));
  const byName = new Map<string, RawSessionRow>();
  for (const rows of bodies) for (const r of rows) byName.set(r.name, r);
  return [...byName.values()];
}
