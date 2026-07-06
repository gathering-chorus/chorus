/* eslint-disable security/detect-object-injection -- indexes parsed-log objects by known field keys, never untrusted input (#3429) */
// #3429 — safe stringify for unknown parsed-log fields (no "[object Object]").
// (declared at top so the parse helpers below can share it.)
// #2840 — typed agent surface for log + error investigation. Wraps Loki HTTP
// API at localhost:3102 with structured input + structured output + a typed
// refusal taxonomy. Earns its keep on top of #2857's trace_id + card_id
// propagation: agents query by trace_id or card_id and the substrate returns
// the full flow as structured rows, not blobs.

export type LogRow = {
  ts: string;
  event?: string;
  role?: string;
  level?: string;
  card_id?: number;
  trace_id?: string;
  branch?: string;
  source?: string;
  raw?: string;
  payload: Record<string, unknown>;
};

export type LogsQueryResult =
  | { ok: true; events: LogRow[]; count: number; truncated: boolean }
  | { ok: false; reason: LogsRefusal; detail?: string };

export type LogsRefusal =
  | 'loki-unreachable'
  | 'query-syntax-error'
  | 'time-range-invalid'
  | 'result-too-large'
  | 'rate-limited';

// #3621 — '7d': a card's werk trace matters for days (the #3609 trace aged out
// of 1d before anyone could view it folded). Week-wide is still one bounded query.
export type TimeWindow = '5m' | '15m' | '1h' | '6h' | '1d' | '7d';

export type LogsQueryDeps = {
  fetchImpl: typeof fetch;
  lokiUrl: string;
  now: () => number;
  // #3149-fix — read-time domain resolver (card_id -> live domain via board-cache).
  // Optional; when absent the rollup leaves domain empty (back-compat / tests).
  domainOf?: (cardId: string) => string | undefined;
};

const TIME_WINDOW_SECONDS: Record<TimeWindow, number> = {
  '5m': 300,
  '15m': 900,
  '1h': 3600,
  '6h': 21600,
  '1d': 86400,
  '7d': 604800,
};

function resolveTimeRange(deps: LogsQueryDeps, start?: string, end?: string, window?: TimeWindow): { startNs: string; endNs: string } | { error: string } {
  const nowMs = deps.now();
  let startMs: number;
  let endMs: number;
  if (start && end) {
    startMs = Date.parse(start);
    endMs = Date.parse(end);
    if (isNaN(startMs) || isNaN(endMs)) return { error: 'unparseable timestamp' };
    if (endMs <= startMs) return { error: 'end must be after start' };
  } else if (window) {
    endMs = nowMs;
    startMs = nowMs - TIME_WINDOW_SECONDS[window] * 1000;
  } else {
    endMs = nowMs;
    startMs = nowMs - 3600 * 1000;
  }
  return { startNs: String(startMs * 1_000_000), endNs: String(endMs * 1_000_000) };
}

// #3031: parse Loki lines, KEEPING non-JSON file-tailed lines instead of dropping
// them. Loki tails raw text (nightly SUITE|… results, coverage, daemon logs)
// alongside JSON spine events; the old `catch { return null }` silently dropped
// every raw line, making file-tailed data invisible through this tool (224 in raw
// Loki vs 0 here). Non-JSON lines now return { ts, raw } so they're queryable.
// tsNs is Loki's own entry timestamp — the truth for raw lines (which carry no ts).
function parseLokiLine(line: string, tsNs: string): LogRow {
  const ms = Number(tsNs) / 1_000_000;
  const lokiTs = Number.isFinite(ms) ? new Date(ms).toISOString() : new Date().toISOString();
  try {
    const obj = JSON.parse(line) as Record<string, unknown>;
    const { ts, event, role, level, card_id, trace_id, branch, source, ...rest } = obj;
    return {
      ts: typeof ts === 'string' ? ts : lokiTs,
      event: typeof event === 'string' ? event : undefined,
      role: typeof role === 'string' ? role : undefined,
      level: typeof level === 'string' ? level : undefined,
      card_id: typeof card_id === 'number' ? card_id : undefined,
      trace_id: typeof trace_id === 'string' ? trace_id : undefined,
      branch: typeof branch === 'string' ? branch : undefined,
      source: typeof source === 'string' ? source : undefined,
      payload: rest,
    };
  } catch {
    return { ts: lokiTs, raw: line, payload: {} };
  }
}

async function executeLokiQuery(query: string, startNs: string, endNs: string, limit: number, deps: LogsQueryDeps): Promise<LogsQueryResult> {
  const params = new URLSearchParams({ query, start: startNs, end: endNs, limit: String(limit) });
  const url = `${deps.lokiUrl}/loki/api/v1/query_range?${params}`;
  let res: Response;
  try {
    res = await deps.fetchImpl(url, { signal: AbortSignal.timeout(8000) });
  } catch (err) {
    return { ok: false, reason: 'loki-unreachable', detail: err instanceof Error ? err.message : String(err) };
  }
  if (!res.ok) {
    const body = await res.text().catch(() => '');
    if (res.status === 400) return { ok: false, reason: 'query-syntax-error', detail: body.slice(0, 500) };
    if (res.status === 429) return { ok: false, reason: 'rate-limited', detail: body.slice(0, 200) };
    return { ok: false, reason: 'loki-unreachable', detail: `HTTP ${res.status}: ${body.slice(0, 200)}` };
  }
  const body = await res.json() as { data?: { result?: Array<{ values?: Array<[string, string]> }> } };
  const events: LogRow[] = [];
  for (const stream of body.data?.result ?? []) {
    for (const [tsNs, line] of stream.values ?? []) {
      events.push(parseLokiLine(line, tsNs));
    }
  }
  events.sort((a, b) => a.ts.localeCompare(b.ts));
  return { ok: true, events, count: events.length, truncated: events.length >= limit };
}

const TIME_RANGE_INVALID = 'time-range-invalid' as const;

export async function queryLogs(
  args: { query: string; start?: string; end?: string; time_window?: TimeWindow; limit?: number },
  deps: LogsQueryDeps,
): Promise<LogsQueryResult> {
  const limit = Math.min(args.limit ?? 100, 1000);
  const range = resolveTimeRange(deps, args.start, args.end, args.time_window);
  if ('error' in range) return { ok: false, reason: TIME_RANGE_INVALID, detail: range.error };
  return executeLokiQuery(args.query, range.startNs, range.endNs, limit, deps);
}

export async function recentErrors(
  args: { role?: string; time_window?: TimeWindow },
  deps: LogsQueryDeps,
): Promise<LogsQueryResult> {
  const window = args.time_window ?? '1h';
  let query = '{job=~".+"} |= "\\"level\\":\\"error\\""';
  if (args.role) query += ` |= "\\"role\\":\\"${args.role}\\""`;
  const range = resolveTimeRange(deps, undefined, undefined, window);
  if ('error' in range) return { ok: false, reason: TIME_RANGE_INVALID, detail: range.error };
  return executeLokiQuery(query, range.startNs, range.endNs, 200, deps);
}

export async function logsForCard(
  args: { card_id: number; time_window?: TimeWindow },
  deps: LogsQueryDeps,
): Promise<LogsQueryResult> {
  const window = args.time_window ?? '1d';
  const query = `{job=~".+"} |~ "\\"card_id\\":${args.card_id}\\\\b"`;
  const range = resolveTimeRange(deps, undefined, undefined, window);
  if ('error' in range) return { ok: false, reason: TIME_RANGE_INVALID, detail: range.error };
  return executeLokiQuery(query, range.startNs, range.endNs, 1000, deps);
}

export async function logsForTrace(
  args: { trace_id: string; time_window?: TimeWindow },
  deps: LogsQueryDeps,
): Promise<LogsQueryResult> {
  const window = args.time_window ?? '1h';
  // #2860 — JSON-field anchor (mirrors logsForCard) so matches are structural,
  // not pure substring. Without this, nudge bodies and tool_call telemetry
  // that literally quote the UUID get pulled in, distorting the per-trace
  // count. The envelope contract in #2839 places trace_id as a top-level
  // field; this query reads against that shape.
  const query = `{job=~".+"} |~ "\\"trace_id\\":\\"${args.trace_id}\\""`;
  const range = resolveTimeRange(deps, undefined, undefined, window);
  if ('error' in range) return { ok: false, reason: TIME_RANGE_INVALID, detail: range.error };
  return executeLokiQuery(query, range.startNs, range.endNs, 1000, deps);
}

// #3023 — query by the git surface work actually ran on. branch is the third
// observability key: card_id = the chain across actions, trace_id = one action,
// branch = where it ran. JSON-field anchor (mirrors logsForTrace) so a branch
// name appearing in a nudge body or commit message doesn't distort the count.
// Catches card-vs-werk divergence (a step that ran on the wrong werk shows the
// wrong branch). Default 1d — branch lifespan is a card's lifetime, not an hour.
export async function logsForBranch(
  args: { branch: string; time_window?: TimeWindow },
  deps: LogsQueryDeps,
): Promise<LogsQueryResult> {
  const window = args.time_window ?? '1d';
  const query = `{job=~".+"} |~ "\\"branch\\":\\"${args.branch}\\""`;
  const range = resolveTimeRange(deps, undefined, undefined, window);
  if ('error' in range) return { ok: false, reason: TIME_RANGE_INVALID, detail: range.error };
  return executeLokiQuery(query, range.startNs, range.endNs, 1000, deps);
}

// #3029 — pain rollup. The fix-death-spiral cure: spine failures grouped by
// class (role · event · reason) ranked by count, so a role sees its pain in
// aggregate instead of fixing blind, one red line at a time. ONE implementation,
// here, behind the chorus-api endpoint + the /borg/pain.html board (:3340).
// Deliberately NOT copied into the mcp-server twin — an MCP tool would be a
// second implementation kept in sync by hand (rots). When the tool is wanted it
// comes from extracting this into one shared module both packages import (the
// #3031 no-competing-implementations follow-up), never a copy. Validated
// contract: {job=~".+"} selector (NOT appName — appName is blind to job-labeled
// spine events, returns 0) + event-FIELD anchor (NOT substring — substring
// over-counts ~6x). Cross-checked vs the live board 2026-05-21: this exact
// grouping reproduces it (kade/demo.show.failed/no_demo_started 432,
// session.context.error 283/125/120, crawler.domain.failed 327...).
// #3029 v1.1 — freeform window (30m / 6h / 12h / 1d / 7d / 30d), matching the
// proven :8899 sketch. Validated by regex; bad input → time-range-invalid.
export type RollupWindow = string;
const WINDOW_RE = /^(\d+)([smhd])$/;
const WINDOW_MULT: Record<string, number> = { s: 1, m: 60, h: 3600, d: 86400 };
export function windowToSeconds(w: string): number | null {
  const m = WINDOW_RE.exec(w);
  return m ? parseInt(m[1], 10) * WINDOW_MULT[m[2] as keyof typeof WINDOW_MULT] : null;
}

// Raw-line failure filter (ported verbatim from logform.py / :8899): match lines
// whose `event` VALUE ends in .refused/.failed/.error. Anchored on the event
// field — NOT a bare substring, which false-counts the pulse heartbeat (~484/day)
// and Grafana self-logs ~6x. We fetch the raw lines and group client-side so each
// class keeps its cards / latest / sample-detail (v1's server-side count threw
// those away — that's why the board was "barely like" the sketch).
// #3165 — widened from event-suffix-only to also key on disposition (deny/refuse/
// rollback) and the .rolledback suffix (one Loki OR-regex, field-anchored). Mirrored
// by lineMatchesPainFilter below. (level=error excluded — see PAIN_DISPOSITIONS note.)
const FAIL_QUERY = '{job=~".+"} |~ `("event":"[^"]*(\\.refused|\\.failed|\\.error|\\.rolledback)")|("disposition":"(deny|refuse|rollback)")`';

// Synthetic/test card ids excluded so a test card never ranks #1 (99998 = the
// show-gate test-card sentinel; demo.show.failed for it was ~85% of the headline).
const SYNTHETIC_CARD_IDS = new Set(['99998', '99999']);

export type PainClass = {
  role: string; event: string; reason: string;
  count: number; cards: string[]; latest: string; detail: string; product: string;
  // #3149-fix — domain DERIVED at read time from the event's card_id via the live
  // board (board-cache), not stamped on the event. Empty when no card_id / unknown
  // card. Part of the group key so failures split by domain, not merge across them.
  domain: string;
};
export type PainRollupResult =
  | { ok: true; window: string; total: number; classes: PainClass[]; byProduct: Record<string, number> }
  | { ok: false; reason: LogsRefusal; detail?: string };

// The failure-suffix rule that defines "pain": a log line counts only if its
// `event` value ends in one of these. Excludes the high-volume non-failures the
// original substring filter false-counted (heartbeat.probe ~484/day,
// system.heartbeat, clearing.probe.passed, Grafana self-logs).
// #3281 — `.blocked` added so agent-side denials become countable. Today the only
// `.blocked` event is `card.quality.blocked` (AC/quality gate denials on card add —
// the "AC-checkbox block"), which carries role + gate=<reason> and was invisible to
// the pain board. Verified no high-volume `.blocked` event exists to false-count.
export const PAIN_EVENT_SUFFIXES = ['.failed', '.refused', '.error', '.rolledback', '.blocked'] as const;
// #3165 — failure DISPOSITIONS counted regardless of event-name suffix. The emit
// slice (werk-pull/commit/push refusals, gate denies, deploy rollbacks) tags the
// disposition without always naming the event `*.refused`; a suffix-only filter
// counted none of them. Field-anchored ("disposition":"deny"), NOT substring —
// preserves the #3029 anchor that excludes the pulse heartbeat (~484/day) and
// grafana self-logs. These are Chorus SPINE events (role·event·card_id), so they
// group cleanly in the rollup.
//
// level=error deliberately NOT included here: live Loki (2026-05-31, 7d) showed 166
// of 193 level=error lines are gathering-app/daemon logs with appName/component/
// message and NO event/role/card_id — they'd all collapse into one "? · '' · ''"
// junk class that dominates the board. Surfacing app-level errors needs its own
// grouping (by component/message), a separate follow-on — NOT bolted onto the
// spine-event rollup. (See #3165 card note.)
export const PAIN_DISPOSITIONS = ['deny', 'refuse', 'rollback'] as const;
// eslint-disable-next-line security/detect-non-literal-regexp -- built from the PAIN_EVENT_SUFFIXES module constant, never user input
const PAIN_LINE_RE = new RegExp(`"event":"[^"]*(${PAIN_EVENT_SUFFIXES.map((s) => s.replace('.', '\\.')).join('|')})"`);
// eslint-disable-next-line security/detect-non-literal-regexp -- built from the PAIN_DISPOSITIONS module constant, never user input
const PAIN_DISPOSITION_RE = new RegExp(`"disposition":"(${PAIN_DISPOSITIONS.join('|')})"`);

// Pure predicate: does a raw Loki line match the pain filter? Mirrors FAIL_QUERY
// so the regression test can prove the exclusion (pulse heartbeat / grafana) on a
// fixture without a live Loki. A line counts if its event ends in a pain suffix
// (incl. .rolledback) OR it carries a failure disposition (deny/refuse/rollback).
export function lineMatchesPainFilter(line: string): boolean {
  return PAIN_LINE_RE.test(line) || PAIN_DISPOSITION_RE.test(line);
}

// One-line "Sample detail" for a failure (ported from logform.py detail()):
// reason, first line of detail, error, then a few high-signal fields.
function str(v: unknown): string {
  if (v == null) return '';
  if (typeof v === 'string') return v;
  if (typeof v === 'number' || typeof v === 'boolean') return String(v);
  return JSON.stringify(v);
}
function sampleDetail(o: Record<string, unknown>): string {
  const bits: string[] = [];
  const s = (k: string): string => str(o[k]);
  if (s('reason')) bits.push(s('reason'));
  if (s('detail')) bits.push(s('detail').trim().split('\n')[0].slice(0, 220));
  if (s('error')) bits.push('err: ' + s('error').slice(0, 160));
  for (const k of ['sha', 'pr_url', 'message_subject', 'file_count', 'paths_count', 'duration_ms', 'status', 'result']) {
    if (s(k)) bits.push(`${k}=${s(k).slice(0, 70)}`);
  }
  return bits.join(' · ');
}

// MM-DD HH:MM:SS (UTC) from a Loki ns timestamp.
function utcStamp(tsNs: string): string {
  const ms = Number(tsNs) / 1e6;
  return Number.isFinite(ms) ? new Date(ms).toISOString().replace('T', ' ').slice(5, 19) : '';
}

// Product attribution (per-product split AC). Gathering = the harvest/crawl/media
// domains; everything else is Chorus (the coordination substrate). Heuristic on
// the event name until a real product tag lands on spine events.
const GATHERING_EVENT_RE = /^(crawler|harvest|photos|music|stories|seeds|garden)[._]|\.(crawl|harvest)\b/;
function productOf(event: string): string {
  return GATHERING_EVENT_RE.test(event) ? 'Gathering' : 'Chorus';
}

// Group raw Loki query_range streams into ranked pain classes. Pure — no I/O, so
// the regression test drives it with a fixture. Each class carries count, the set
// of cards, the latest occurrence, and a sample detail (the actionable context).
// Synthetic test cards are dropped so they never rank #1; per-product totals are
// accumulated alongside.
export function rollupRawLines(
  body: unknown,
  opts?: { role?: string },
  // #3149-fix — read-time domain resolver: card_id -> live domain (board-cache).
  // Optional so the pure regression tests run without a board (domain stays '').
  domainOf?: (cardId: string) => string | undefined,
): { classes: PainClass[]; total: number; byProduct: Record<string, number> } {
  const result = (body as { data?: { result?: Array<{ values?: Array<[string, string]> }> } } | undefined)?.data?.result ?? [];
  const roll = new Map<string, PainAcc>();
  let total = 0;
  const byProduct: Record<string, number> = {};
  for (const stream of result) {
    for (const [tsNs, line] of stream.values ?? []) {
      total += accumulatePainLine(tsNs, line, opts, roll, byProduct);
    }
  }
  const classes: PainClass[] = [...roll.values()]
    .map((a) => {
      const cards = [...a.cards].sort();
      // Derive domain from the class's representative card via the live board —
      // not stamped on the event, never stale. Empty when no card / unknown card.
      const domain = cards[0] && domainOf ? domainOf(cards[0]) ?? '' : '';
      return { role: a.role, event: a.event, reason: a.reason, count: a.count, cards, latest: utcStamp(a.latestNs), detail: a.detail, product: a.product, domain };
    })
    .sort((a, b) => b.count - a.count);
  return { classes, total, byProduct };
}

type PainAcc = { role: string; event: string; reason: string; count: number; latestNs: string; detail: string; cards: Set<string>; product: string };

// One Loki line accumulated into the rollup. Returns 1 if counted, 0 if skipped
// (parse failure, synthetic card, role filter). Split out of rollupRawLines to
// hold that function under the complexity ceiling.
type PainFields = { o: Record<string, unknown>; cardId: string; role: string; event: string; reason: string; product: string };

// Parse one Loki line into pain fields, or null if it should be skipped
// (parse failure, synthetic test card, role filter).
function parsePainLine(line: string, roleFilter: string | undefined): PainFields | null {
  let o: Record<string, unknown>;
  try { o = JSON.parse(line) as Record<string, unknown>; } catch { return null; }
  const cardId = str(o.card_id) || str(o.card);
  if (cardId && SYNTHETIC_CARD_IDS.has(cardId)) return null; // exclude synthetic test cards
  const role = str(o.role) || '?';
  if (roleFilter && role !== roleFilter) return null;
  const event = str(o.event);
  const reason = str(o.reason ?? o.error).slice(0, 60);
  return { o, cardId, role, event, reason, product: productOf(event) };
}

function accumulatePainLine(
  tsNs: string,
  line: string,
  opts: { role?: string } | undefined,
  roll: Map<string, PainAcc>,
  byProduct: Record<string, number>,
): number {
  const f = parsePainLine(line, opts?.role);
  if (!f) return 0;
  const key = `${f.role} ${f.event} ${f.reason}`;
  let acc = roll.get(key);
  if (!acc) { acc = { role: f.role, event: f.event, reason: f.reason, count: 0, latestNs: '0', detail: '', cards: new Set(), product: f.product }; roll.set(key, acc); }
  acc.count++;
   
  byProduct[f.product] = (byProduct[f.product] ?? 0) + 1;
  if (tsNs > acc.latestNs) { acc.latestNs = tsNs; acc.detail = sampleDetail(f.o); }
  if (f.cardId) acc.cards.add(f.cardId);
  return 1;
}

export async function queryPainRollup(args: { window?: string; role?: string }, deps: LogsQueryDeps): Promise<PainRollupResult> {
  const window = args.window ?? '7d';
  const secs = windowToSeconds(window);
  if (secs === null) return { ok: false, reason: TIME_RANGE_INVALID, detail: `window must match <n>[smhd] (e.g. 12h, 1d, 7d); got ${String(window)}` };
  const now = deps.now();
  const params = new URLSearchParams({
    query: FAIL_QUERY,
    start: String((now - secs * 1000) * 1_000_000),
    end: String(now * 1_000_000),
    limit: '5000',
    direction: 'backward',
  });
  const url = `${deps.lokiUrl}/loki/api/v1/query_range?${params}`;
  // Cold-connect to Loki fails transiently (~1 in 3 — observed live); retry a few
  // times with small backoff. Only a genuinely-down Loki returns the refusal.
  let res: Response | undefined;
  let lastErr: unknown;
  for (let attempt = 0; attempt < 3; attempt++) {
    try {
      res = await deps.fetchImpl(url, { signal: AbortSignal.timeout(15000) });
      break;
    } catch (err) {
      lastErr = err;
      await new Promise((r) => setTimeout(r, 250 * (attempt + 1)));
    }
  }
  if (!res) {
    return { ok: false, reason: 'loki-unreachable', detail: lastErr instanceof Error ? lastErr.message : String(lastErr) };
  }
  if (!res.ok) {
    const body = await res.text().catch(() => '');
    if (res.status === 400) return { ok: false, reason: 'query-syntax-error', detail: body.slice(0, 500) };
    if (res.status === 429) return { ok: false, reason: 'rate-limited', detail: body.slice(0, 200) };
    return { ok: false, reason: 'loki-unreachable', detail: `HTTP ${res.status}: ${body.slice(0, 200)}` };
  }
  const { classes, total, byProduct } = rollupRawLines(await res.json(), { role: args.role }, deps.domainOf);
  return { ok: true, window, total, classes, byProduct };
}
