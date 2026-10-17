// @domain: messages
// #4571 — Jeff's room message reaches the role the way the current Clearing does it
// (deliverJeffMessageToTarget): POST pulse /api/jeff-input with the Clearing caller
// header. Not /api/nudge: that is the role-to-role path and refuses any caller but
// the nudge sender (#3485), which v2 hit as a silent 403 (Abby's catch, 20:08).
// A refused hand-off is reported, never swallowed.
export type FetchLike = (url: string, init: { method: string; headers: Record<string, string>; body: string; signal?: AbortSignal }) => Promise<{ ok: boolean; status: number; text(): Promise<string> }>;

export async function deliverJeffInput(
  pulseUrl: string, to: string, content: string, fetchFn: FetchLike = fetch as unknown as FetchLike,
): Promise<{ ok: true } | { ok: false; reason: string }> {
  const ctrl = new AbortController();
  const timer = setTimeout(() => ctrl.abort(), 5000);
  try {
    const res = await fetchFn(`${pulseUrl}/api/jeff-input`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'X-Chorus-Clearing-Caller': '1' },
      body: JSON.stringify({ to, content }),
      signal: ctrl.signal,
    });
    if (!res.ok) return { ok: false, reason: `pulse ${res.status}: ${(await res.text().catch(() => '')).slice(0, 160)}` };
    return { ok: true };
  } catch (e) {
    return { ok: false, reason: (e instanceof Error ? e.message : String(e)).split('\n')[0] };
  } finally {
    clearTimeout(timer);
  }
}
