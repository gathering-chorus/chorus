/**
 * #3818 — the word cap: counting, and resolving the limit.
 *
 * Jeff, 2026-08-11: "seriously contemplating a hard cap of 100 words on all
 * agent responses" — "including chats between u." The cap exists because three
 * roles emit far more than he can read, and the volume costs him the attention
 * that should go to thinking.
 *
 * TWO THINGS LIVE HERE and they are deliberately separable:
 *   countWords  — pure, no I/O, must match the Rust implementation exactly
 *                 (both run config/word-cap-fixtures.json)
 *   resolveCap  — reads the cap from the graph, cached, FAIL-OPEN
 */

/** Words in the PROSE of a message. See config/word-cap-fixtures.json for the
 *  authoritative case list — this function and Kade's Rust one both run it.
 *
 *  Order matters: fenced blocks first (they may contain backticks and URLs
 *  that would otherwise be stripped as inline code and leave prose behind),
 *  then inline code, then URLs. */
export function countWords(text: string): number {
  const withoutFences = text.replace(/```[\s\S]*?```/g, ' ')
    // An UNCLOSED fence strips to the end. That is the safe direction: the
    // alternative counts a pasted log as prose and bounces a message for
    // showing evidence.
    .replace(/```[\s\S]*$/g, ' ');
  const withoutInline = withoutFences.replace(/`[^`\n]*`/g, ' ');
  // ws:// and wss:// alongside http(s) — Kade's catch: Buzz relay URLs already
  // appear in our nudges, and a relay address is exactly the kind of unreadable
  // token that should not spend Jeff's word budget.
  const withoutUrls = withoutInline.replace(/\b(?:https?|wss?):\/\/\S+/gi, ' ');
  // A "word" needs at least one letter or digit — bare punctuation and a lone
  // emoji are not words. Without this, an em-dash between clauses counts.
  return withoutUrls
    .split(/\s+/)
    .filter((t) => /[\p{L}\p{N}]/u.test(t))
    .length;
}

/** Two caps, two keys, because they are two different acts.
 *
 *  A RESPONSE is Jeff reading an answer — 100 is his number, from
 *  2026-08-11: "seriously contemplating a hard cap of 100 words on all agent
 *  responses."
 *
 *  A NUDGE is one role asking another for one thing. Jeff, same message:
 *  "including chats between u", and the card says nudges cap tighter. Half the
 *  response budget is enough for an ask plus the reason it matters, and the
 *  tighter number is the point — role-to-role traffic is the bloat Jeff never
 *  sees and never asked for. */
export const CAP_KEY_RESPONSE = 'response.word.cap';
export const CAP_KEY_NUDGE = 'nudge.word.cap';

/** The cap when the graph cannot be reached. NOT a policy decision — a
 *  safety one. See resolveCap. */
export const FAIL_OPEN_CAP = 100;
export const FAIL_OPEN_NUDGE_CAP = 50;

/** The fail-open default for a key. A key we do not know defaults to the
 *  looser value — an unknown surface must not be the strictest one. */
export function failOpenFor(key: string): number {
  return key === CAP_KEY_NUDGE ? FAIL_OPEN_NUDGE_CAP : FAIL_OPEN_CAP;
}

/** How long a resolved cap is trusted. Long enough that the store is not on
 *  the hot path of every message; short enough that Jeff editing the property
 *  takes effect while he is still thinking about it. */
export const CAP_TTL_MS = 60_000;

type CacheEntry = { cap: number; at: number };
// Keyed by key+role: the nudge cap and the response cap for the same role are
// different values, and a single-key cache would serve one for the other.
const cache = new Map<string, CacheEntry>();

/** Test seam — the clock and the fetch, so a TTL test does not sleep. */
export type ResolveDeps = {
  fetchImpl: (url: string) => Promise<{ ok: boolean; json: () => Promise<unknown> }>;
  now: () => number;
  apiBase?: string;
};

/**
 * Resolve the word cap for a role from the Properties domain.
 *
 * FAIL-OPEN, and this is the load-bearing decision (Kade's catch, #3818):
 * putting the graph on the send path means a Fuseki outage could reject every
 * message the team sends — muting all three roles, the same shape as the
 * hooks-daemon lockout where a dead dependency became a team-wide stop. A cap
 * is a courtesy to Jeff, not a security control. So every failure path —
 * unreachable, timeout, malformed, missing property — returns the default and
 * lets the message through. The worst case must be "we were wordy today",
 * never "the team went silent."
 */
export async function resolveCap(
  role: string,
  deps: ResolveDeps,
  key: string = CAP_KEY_RESPONSE,
): Promise<number> {
  const fallback = failOpenFor(key);
  const cacheKey = `${key}|${role}`;
  const hit = cache.get(cacheKey);
  if (hit && deps.now() - hit.at < CAP_TTL_MS) return hit.cap;

  const base = deps.apiBase ?? process.env.CHORUS_API_URL ?? 'http://localhost:3340';
  try {
    const resp = await deps.fetchImpl(
      `${base}/api/chorus/properties/resolve?key=${encodeURIComponent(key)}&scope=role&name=${encodeURIComponent(role)}`,
    );
    if (!resp.ok) return fallback;
    const body = (await resp.json()) as { value?: unknown } | null;
    const n = Number(body?.value);
    // A non-positive or absurd cap is a misconfiguration, not an instruction.
    // Refusing to honour it here means a fat-fingered "0" cannot mute the team.
    if (!Number.isFinite(n) || n <= 0) return fallback;
    // A failure is NOT cached: the fallback is a stand-in for an answer we did
    // not get, and caching it would keep serving it after the store recovers.
    cache.set(cacheKey, { cap: n, at: deps.now() });
    return n;
  } catch {
    return fallback;
  }
}

/** Test seam only — the cache is process-global by design. */
export function __clearCapCache(): void {
  cache.clear();
}

export type CapVerdict =
  | { ok: true }
  | { ok: false; words: number; cap: number; message: string };

/**
 * The gate. Over-cap is REJECTED WITH THE COUNT, never silently truncated —
 * truncation would cut Jeff's answer mid-sentence and hide that it happened.
 */
export function checkCap(text: string, cap: number): CapVerdict {
  const words = countWords(text);
  if (words <= cap) return { ok: true };
  return {
    ok: false,
    words,
    cap,
    message:
      `word-cap: ${words} words, cap is ${cap}. Rewrite shorter and resend — ` +
      `nothing was sent and nothing was truncated. Fenced code blocks, inline code, ` +
      `and URLs do not count toward the total, so receipts belong in a fence.`,
  };
}
