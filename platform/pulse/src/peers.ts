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

/**
 * #4692 — the roles door read the way a nudge reads it, on a timer. On 10-21
 * from 21:46 to 05:53 the door answered curl but gave node and Rust clients
 * its Discovery page, so every nudge was refused and nothing said so. This
 * tick uses fetchPeers itself (pulse's own client path, not curl): a refused
 * read is reported every tick until the door answers again.
 */
export async function rolesDoorTick(
  doFetch?: FetchLike,
  base?: string,
): Promise<{ ok: true; peers: number } | { ok: false; error: string }> {
  try {
    return { ok: true, peers: (await fetchPeers(base, doFetch)).length };
  } catch (e) {
    return { ok: false, error: (e as Error).message };
  }
}

export function startRolesDoorWatch(
  onFail: (error: string) => Promise<void> | void,
  intervalMs = 60_000,
  doFetch?: FetchLike,
): () => void {
  const timer = setInterval(() => {
    void (async () => {
      const r = await rolesDoorTick(doFetch);
      if (!r.ok) {
        try { await onFail(r.error); } catch { /* a failed report never stops the watch */ }
      }
    })();
  }, intervalMs);
  timer.unref?.();
  return () => clearInterval(timer);
}
