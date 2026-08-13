/**
 * #3844 — the nudge transport, behind one interface.
 *
 * Jeff, 2026-08-13: "nudge becomes a facade for buzz right?" Yes — and this is
 * the step that makes that a choice instead of a rewrite.
 *
 * WHAT THIS IS NOT: it is not the Buzz decision. Today there is exactly one
 * implementation and it does what executeNudge already did — POST to pulse,
 * which drives the terminal injector. Nothing observable changes. The point is
 * that "which transport" becomes a value rather than the shape of the code.
 *
 * WHY IT MATTERS, from the night before this card: a hard-coded tmux buffer
 * name inside that transport silently delivered one role's message to another
 * role's terminal and dropped the second. Nobody could see it because there was
 * no seam — the transport WAS executeNudge, so there was nowhere to stand and
 * ask "did it arrive?" A facade is where delivery semantics, fallback, and
 * acknowledgement can live at all.
 */

/** What a transport must answer. Not "did I try" — what happened. */
export type SendResult =
  | { ok: true; resolved: string }
  | { ok: false; error: string };

export type NudgePayload = {
  from: string;
  to: string;
  content: string;
  traceId: string;
  expects: string;
};

export interface NudgeTransport {
  /** Stable name for logs, witnesses, and the failure message. */
  readonly name: string;
  send(payload: NudgePayload): Promise<SendResult>;
}

/**
 * The fetch seam. This is DELIBERATELY the same shape as server.ts's FetchImpl,
 * not a looser one.
 *
 * The first version declared `init: Record<string, unknown>` and a required
 * `status`, which forced `fetchImpl as unknown as FetchLike` at the call site —
 * a double-cast, flagged by both Silas and Kade. A double-cast does not fix a
 * type mismatch, it hides one: it would have kept compiling if the shapes drifted
 * apart, which is how a transport starts reading a field that is never there.
 */
export type FetchLike = (
  url: string,
  init?: { method?: string; headers?: Record<string, string>; body?: string; signal?: AbortSignal },
) => Promise<{
  ok: boolean;
  status?: number;
  json: () => Promise<unknown>;
  text?: () => Promise<string>;
}>;

/**
 * The transport that exists today: POST to pulse, which owns delivery.
 *
 * Extracted verbatim from executeNudge — same URL resolution, same headers,
 * same 5s abort, same resolved-destination parse. A behaviour change here would
 * make this card two changes instead of one, and the second one unreviewed.
 */
export class PulseTransport implements NudgeTransport {
  readonly name = 'pulse';

  constructor(
    private fetchImpl: FetchLike,
    private url: string,
    private secret: string | undefined,
    private timeoutMs = 5000,
  ) {}

  async send(p: NudgePayload): Promise<SendResult> {
    const ctrl = new AbortController();
    const timeoutId = setTimeout(() => ctrl.abort(), this.timeoutMs);
    try {
      const headers: Record<string, string> = {
        'Content-Type': 'application/json',
        'X-Chorus-Trace-Id': p.traceId,
        'X-Chorus-MCP-Caller': '1',
      };
      if (this.secret) headers['X-Chorus-Pulse-Secret'] = this.secret;
      const resp = await this.fetchImpl(this.url, {
        method: 'POST',
        headers,
        body: JSON.stringify({
          from: p.from,
          to: p.to,
          content: p.content,
          traceId: p.traceId,
          expects: p.expects,
        }),
        signal: ctrl.signal,
      });
      if (!resp.ok) {
        const errText = resp.text ? await resp.text().catch(() => '') : '';
        // status is optional on the seam, so say "unknown" rather than print
        // the word undefined into an operator-facing failure message.
        return {
          ok: false,
          error: `pulse POST returned ${resp.status ?? 'unknown status'}: ${errText.slice(0, 200)}`,
        };
      }
      // #3439 — surface the destination pulse actually resolved, so "sent" is
      // not blind. Best-effort: a parse miss falls back to the bare role.
      let resolved = p.to;
      try {
        const body = (await resp.json()) as { resolved?: string } | null;
        if (body && typeof body.resolved === 'string') resolved = body.resolved;
      } catch {
        /* keep the bare role */
      }
      return { ok: true, resolved };
    } catch (err) {
      return { ok: false, error: err instanceof Error ? err.message : String(err) };
    } finally {
      clearTimeout(timeoutId);
    }
  }
}

/**
 * Try transports in order until one succeeds.
 *
 * THE RULE THIS ENCODES: a facade that swallows a dead transport is worse than
 * no facade. If every transport fails, this fails — loudly, naming each one and
 * why. "Sent" must never become a word that means "we tried."
 *
 * That is not hypothetical caution. The whole reason this card exists is a
 * transport that returned success while delivering the wrong message.
 */
export async function sendVia(
  transports: NudgeTransport[],
  payload: NudgePayload,
): Promise<{ result: SendResult; attempts: Array<{ name: string; error: string }> }> {
  const attempts: Array<{ name: string; error: string }> = [];
  if (transports.length === 0) {
    return {
      result: { ok: false, error: 'no transport configured — nothing was sent' },
      attempts,
    };
  }
  for (const t of transports) {
    const r = await t.send(payload);
    if (r.ok) {
      return { result: r, attempts };
    }
    attempts.push({ name: t.name, error: r.error });
  }
  return {
    result: {
      ok: false,
      error:
        'all transports failed — ' +
        attempts.map((a) => `${a.name}: ${a.error}`).join('; '),
    },
    attempts,
  };
}

/**
 * Which transports, in order — DATA, not a compile-time constant.
 *
 * Same lesson as the word cap it sits next to: changing behaviour should be an
 * edit, not a deploy. CHORUS_NUDGE_TRANSPORTS is a comma-separated ordered
 * list; unknown names are IGNORED and reported by the caller rather than
 * silently dropping to a default, because a typo that quietly reverts you to
 * the old transport is indistinguishable from the change never happening.
 */
export function selectTransports(
  available: Map<string, NudgeTransport>,
  spec: string | undefined,
): { chosen: NudgeTransport[]; unknown: string[] } {
  const names = (spec ?? 'pulse')
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean);
  const chosen: NudgeTransport[] = [];
  const unknown: string[] = [];
  for (const n of names) {
    const t = available.get(n);
    if (t) chosen.push(t);
    else unknown.push(n);
  }
  return { chosen, unknown };
}
