// @domain: messages
// #4875 AC5 — who answers an unaddressed Jeff message: one moderator, read from the global
// property `room.moderator` (Jeff 11-03 07:15: "not hard-coded … a property that's configurable").
// The router delivers such a message to the moderator only; with no valid row, it goes to every
// role as before (#4604), so a missing row never swallows Jeff's words.

// #4889 (Kade 07:06) — read through the one shared TS properties reader, the same rules
// as the Rust resolver: a row that is missing, empty or not global is no moderator.
import { globalValueFromBody, readGlobal } from '../../../platform/chorus-sdk/properties/global';

export const MODERATOR_KEY = 'room.moderator';
const ROLE = /^[a-z][a-z0-9-]{0,31}$/;

/** The moderator a properties-door body names, or undefined. Pure. */
export function moderatorFromProperties(body: unknown): string | undefined {
  return roleOf(globalValueFromBody(body, MODERATOR_KEY));
}

function roleOf(r: { ok: boolean; value?: string }): string | undefined {
  const v = r.ok && r.value ? r.value.toLowerCase() : '';
  return ROLE.test(v) ? v : undefined;
}

/** Reads the property now and every `everyMs`, handing each value to `apply`. `door` is the
 *  properties door (default: the shared reader's, OWL_URL else athena-make). */
export function watchModerator(apply: (m: string | undefined) => void, door?: string,
  everyMs = 60_000, fetchFn: typeof fetch = fetch,
  log: (line: string) => void = (l) => console.error(l)): () => void {
  let stopped = false;
  let lastMissing = false;
  const read = async () => {
    const r = await readGlobal(MODERATOR_KEY, { door, fetchFn });
    if (!r.ok && r.where === 'door') return; // keep the last value; the next read retries
    const m = roleOf(r);
    apply(m);
    // Kade a587a9b70: the fall-open to every role must be visible, once per change, not silent.
    if (!m && !lastMissing) log(JSON.stringify({ level: 'warn', event: 'room.moderator.missing', key: MODERATOR_KEY, ts: new Date().toISOString() }));
    lastMissing = !m;
  };
  void read();
  const t = setInterval(() => { if (!stopped) void read(); }, everyMs);
  if (typeof t.unref === 'function') t.unref();
  return () => { stopped = true; clearInterval(t); };
}
