// @domain: messages
/**
 * #4432 — who may hold a conversation: every role the roles door lists as an
 * agent or a human (Jeff, Wren, Silas, Kade, and now Abby Normal). This is the
 * one place pulse learns the peer set; nothing here lists names by hand. An
 * unreadable door is an error the caller must surface, never "the usual four".
 */
export type FetchLike = (url: string, init?: { signal?: AbortSignal }) => Promise<{ ok: boolean; status: number; json(): Promise<unknown> }>;

export function peersFrom(body: unknown): string[] {
  const rows = (body as { data?: unknown } | null)?.data;
  if (!Array.isArray(rows)) throw new Error('the roles door answered with no data list');
  const names = rows
    .filter((r) => r && ['agent', 'human'].includes((r as { roleKind?: string }).roleKind ?? ''))
    .map((r) => String((r as { name?: string }).name ?? ''))
    .filter(Boolean)
    .sort();
  if (names.length === 0) throw new Error('the roles door lists no agent or human role');
  return names;
}

export async function fetchPeers(
  base = process.env.ATHENA_MAKE_URL || 'http://localhost:3360',
  doFetch: FetchLike = fetch as unknown as FetchLike,
): Promise<string[]> {
  const url = `${base}/v1/roles/roles?limit=500`;
  const r = await doFetch(url, { signal: AbortSignal.timeout(2000) });
  if (!r.ok) throw new Error(`the roles door answered HTTP ${r.status} (${url})`);
  return peersFrom(await r.json());
}
