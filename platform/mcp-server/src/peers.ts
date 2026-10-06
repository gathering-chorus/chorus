// @domain: messages
/**
 * #4432 — who may send or receive a nudge, and which roles may run an MCP
 * session: the roles door (`/v1/roles/roles`), never a list in this package.
 * `peers` = every agent or human role (Jeff, Wren, Silas, Kade, Abby Normal);
 * `agents` = the agent roles only. A door that does not answer is an error the
 * caller surfaces; there is no fallback to the old names.
 */
export interface RoleSets { peers: string[]; agents: string[] }
type FetchLike = (url: string, init?: { signal?: AbortSignal }) => Promise<{ ok: boolean; status: number; json(): Promise<unknown> }>;

export function roleSetsFrom(body: unknown): RoleSets {
  const rows = (body as { data?: unknown } | null)?.data;
  if (!Array.isArray(rows)) throw new Error('the roles door answered with no data list');
  const kind = (r: unknown) => String((r as { roleKind?: string } | null)?.roleKind ?? '');
  const name = (r: unknown) => String((r as { name?: string } | null)?.name ?? '');
  const peers = rows.filter((r) => ['agent', 'human'].includes(kind(r))).map(name).filter(Boolean).sort();
  const agents = rows.filter((r) => kind(r) === 'agent').map(name).filter(Boolean).sort();
  if (agents.length === 0) throw new Error('the roles door lists no agent role');
  return { peers, agents };
}

let cached: { at: number; sets: RoleSets } | null = null;
const TTL_MS = 30_000;

/** Fresh within 30s; an expired entry is re-read, and a failed re-read throws. */
export async function fetchRoleSets(
  base = process.env.ATHENA_MAKE_URL || 'http://localhost:3360',
  doFetch: FetchLike = fetch as unknown as FetchLike,
  now = Date.now(),
): Promise<RoleSets> {
  if (cached && now - cached.at < TTL_MS) return cached.sets;
  const url = `${base}/v1/roles/roles?limit=500`;
  const r = await doFetch(url, { signal: AbortSignal.timeout(2000) });
  if (!r.ok) throw new Error(`the roles door answered HTTP ${r.status} (${url})`);
  const sets = roleSetsFrom(await r.json());
  cached = { at: now, sets };
  return sets;
}

export function resetRoleSetsCache(): void { cached = null; }
