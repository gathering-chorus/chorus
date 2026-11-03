// @domain: messages
// #4875 AC5 — who answers an unaddressed Jeff message: one moderator, read from the global
// property `room.moderator` (Jeff 11-03 07:15: "not hard-coded … a property that's configurable").
// The router delivers such a message to the moderator only; with no valid row, it goes to every
// role as before (#4604), so a missing row never swallows Jeff's words.

export const DEFAULT_PROPERTIES_URL = 'http://localhost:3340/owl/v1/properties/properties';
export const MODERATOR_KEY = 'room.moderator';
const ROLE = /^[a-z][a-z0-9-]{0,31}$/;

/** The moderator a properties-door body names, or undefined. Pure. */
export function moderatorFromProperties(body: unknown): string | undefined {
  const rows = (body as { data?: unknown } | null)?.data;
  if (!Array.isArray(rows)) return undefined;
  const row = (rows as Array<Record<string, unknown>>).find((r) => r.propertyKey === MODERATOR_KEY);
  const v = typeof row?.propertyValue === 'string' ? row.propertyValue.trim().toLowerCase() : '';
  return ROLE.test(v) ? v : undefined;
}

/** Reads the property now and every `everyMs`, handing each value to `apply`. */
export function watchModerator(apply: (m: string | undefined) => void, url = DEFAULT_PROPERTIES_URL,
  everyMs = 60_000, fetchFn: typeof fetch = fetch): () => void {
  let stopped = false;
  const read = async () => {
    try {
      const res = await fetchFn(url, { signal: AbortSignal.timeout(5000) });
      if (res.ok) apply(moderatorFromProperties(await res.json()));
    } catch { /* keep the last value; the next read retries */ }
  };
  void read();
  const t = setInterval(() => { if (!stopped) void read(); }, everyMs);
  if (typeof t.unref === 'function') t.unref();
  return () => { stopped = true; clearInterval(t); };
}
