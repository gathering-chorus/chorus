// @domain: messages
// #4571 — Jeff's room message reaches the role the way the current Clearing does it
// (deliverJeffMessageToTarget): POST pulse /api/jeff-input with the Clearing caller
// header. Not /api/nudge: that is the role-to-role path and refuses any caller but
// the nudge sender (#3485), which v2 hit as a silent 403 (Abby's catch, 20:08).
// A refused hand-off is reported, never swallowed.
export type FetchLike = (url: string, init: { method: string; headers: Record<string, string>; body: string; signal?: AbortSignal }) => Promise<{ ok: boolean; status: number; text(): Promise<string> }>;

type Delivery = { ok: true } | { ok: false; reason: string };

/** #4649 — socket failures that happen before pulse answers. On 10-19 three hand-offs
 *  died this way ("fetch failed") with no POST in pulse's log, so one retry is safe;
 *  a timeout or any answer from pulse may mean it already has the message. */
const PRE_ANSWER_CODES = new Set(['ECONNRESET', 'ECONNREFUSED', 'EPIPE', 'UND_ERR_SOCKET']);

function causeCode(e: unknown): string | undefined {
  const cause = (e as { cause?: { code?: unknown } } | undefined)?.cause;
  return typeof cause?.code === 'string' ? cause.code : undefined;
}

function describe(e: unknown): string {
  const msg = (e instanceof Error ? e.message : String(e)).split('\n')[0];
  const code = causeCode(e);
  const causeMsg = (e as { cause?: { message?: unknown } } | undefined)?.cause?.message;
  return code ? `${msg} (${code}${typeof causeMsg === 'string' ? `: ${causeMsg}` : ''})` : msg;
}

async function postOnce(pulseUrl: string, to: string, content: string, fetchFn: FetchLike, traceId?: string): Promise<Delivery & { retry?: boolean }> {
  const ctrl = new AbortController();
  const timer = setTimeout(() => ctrl.abort(), 5000);
  try {
    const res = await fetchFn(`${pulseUrl}/api/jeff-input`, {
      method: 'POST',
      // #4649 — a fresh socket per hand-off: pulse hints keep-alive 5 s and fetch
      // closes pooled sockets near that edge, so reuse races a close (Silas, 10-20).
      headers: { 'Content-Type': 'application/json', 'X-Chorus-Clearing-Caller': '1', Connection: 'close' },
      // #4672 — jeff-<signed id>-<role>: pulse's Buzz shadow joins his note to this row by it
      body: JSON.stringify(traceId ? { to, content, traceId } : { to, content }),
      signal: ctrl.signal,
    });
    if (!res.ok) return { ok: false, reason: `pulse ${res.status}: ${(await res.text().catch(() => '')).slice(0, 160)}` };
    return { ok: true };
  } catch (e) {
    const code = causeCode(e);
    return { ok: false, reason: describe(e), retry: code !== undefined && PRE_ANSWER_CODES.has(code) };
  } finally {
    clearTimeout(timer);
  }
}

export async function deliverJeffInput(
  pulseUrl: string, to: string, content: string, fetchFn: FetchLike = fetch as unknown as FetchLike, traceId?: string,
): Promise<Delivery> {
  const first = await postOnce(pulseUrl, to, content, fetchFn, traceId);
  if (first.ok || !first.retry) return first.ok ? { ok: true } : { ok: false, reason: first.reason };
  const second = await postOnce(pulseUrl, to, content, fetchFn, traceId);
  return second.ok ? { ok: true } : { ok: false, reason: `${second.reason}; first try: ${first.reason}` };
}
