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

export type TimeWindow = '5m' | '15m' | '1h' | '6h' | '1d';

export type LogsQueryDeps = {
  fetchImpl: typeof fetch;
  lokiUrl: string;
  now: () => number;
};

const TIME_WINDOW_SECONDS: Record<TimeWindow, number> = {
  '5m': 300,
  '15m': 900,
  '1h': 3600,
  '6h': 21600,
  '1d': 86400,
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

export async function queryLogs(
  args: { query: string; start?: string; end?: string; time_window?: TimeWindow; limit?: number },
  deps: LogsQueryDeps,
): Promise<LogsQueryResult> {
  const limit = Math.min(args.limit ?? 100, 1000);
  const range = resolveTimeRange(deps, args.start, args.end, args.time_window);
  if ('error' in range) return { ok: false, reason: 'time-range-invalid', detail: range.error };
  return executeLokiQuery(args.query, range.startNs, range.endNs, limit, deps);
}

export async function recentErrors(
  args: { role?: string; time_window?: TimeWindow },
  deps: LogsQueryDeps,
): Promise<LogsQueryResult> {
  const window = args.time_window ?? '1h';
  let query = `{job=~".+"} |= "\\"level\\":\\"error\\""`;
  if (args.role) query += ` |= "\\"role\\":\\"${args.role}\\""`;
  const range = resolveTimeRange(deps, undefined, undefined, window);
  if ('error' in range) return { ok: false, reason: 'time-range-invalid', detail: range.error };
  return executeLokiQuery(query, range.startNs, range.endNs, 200, deps);
}

export async function logsForCard(
  args: { card_id: number; time_window?: TimeWindow },
  deps: LogsQueryDeps,
): Promise<LogsQueryResult> {
  const window = args.time_window ?? '1d';
  const query = `{job=~".+"} |~ "\\"card_id\\":${args.card_id}\\\\b"`;
  const range = resolveTimeRange(deps, undefined, undefined, window);
  if ('error' in range) return { ok: false, reason: 'time-range-invalid', detail: range.error };
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
  if ('error' in range) return { ok: false, reason: 'time-range-invalid', detail: range.error };
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
  if ('error' in range) return { ok: false, reason: 'time-range-invalid', detail: range.error };
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
export type RollupWindow = '1d' | '7d' | '30d';
const ROLLUP_WINDOWS: RollupWindow[] = ['1d', '7d', '30d'];

export type PainClass = { role: string; event: string; reason: string; count: number };
export type PainRollupResult =
  | { ok: true; window: RollupWindow; total: number; classes: PainClass[] }
  | { ok: false; reason: LogsRefusal; detail?: string };

// The failure-suffix rule that defines "pain": an event counts only if its name
// carries one of these suffixes. This is what excludes the high-volume
// non-failures the original substring filter false-counted — heartbeat.probe
// (the ~484/day FP), system.heartbeat, clearing.probe.passed, grafana self-logs.
// Mirrored by the LogQL `event=~".+(\.failed|\.refused|\.error)"` in
// buildRollupQuery; the drift-guard test asserts the two stay in sync.
export const PAIN_EVENT_SUFFIXES = ['.failed', '.refused', '.error'] as const;
const PAIN_EVENT_RE = new RegExp(`.+(${PAIN_EVENT_SUFFIXES.map((s) => s.replace('.', '\\.')).join('|')})`);

// Pure predicate form of the rollup's event filter, so the regression test can
// prove the exclusion (pulse heartbeat / grafana self-logs) deterministically
// without a live Loki. Matches what Loki evaluates server-side from the LogQL.
export function eventMatchesPainFilter(event: string): boolean {
  return PAIN_EVENT_RE.test(event);
}

// The single source of the rollup query — MCP tool, HTTP endpoint, and the
// regression test all call this so they can never drift.
export function buildRollupQuery(window: RollupWindow): string {
  return `topk(500, sum by (role, event, reason) (count_over_time(`
    + `{job=~".+"} | json | event=~".+(\\\\.failed|\\\\.refused|\\\\.error)" [${window}])))`;
}

// Parse a Loki instant-vector response into ranked pain classes. Pure — no I/O,
// so the test drives it with a fixture.
export function parseRollupVector(body: unknown): PainClass[] {
  const result = (body as { data?: { result?: Array<{ metric?: Record<string, string>; value?: [number, string] }> } })?.data?.result ?? [];
  const classes: PainClass[] = [];
  for (const row of result) {
    const m = row.metric ?? {};
    const count = row.value ? parseInt(row.value[1], 10) : 0;
    if (!count) continue;
    classes.push({ role: m.role ?? '', event: m.event ?? '', reason: m.reason ?? '', count });
  }
  classes.sort((a, b) => b.count - a.count);
  return classes;
}

export async function queryPainRollup(args: { window?: RollupWindow }, deps: LogsQueryDeps): Promise<PainRollupResult> {
  const window: RollupWindow = args.window ?? '7d';
  if (!ROLLUP_WINDOWS.includes(window)) return { ok: false, reason: 'time-range-invalid', detail: `window must be one of ${ROLLUP_WINDOWS.join('|')}; got ${String(window)}` };
  const params = new URLSearchParams({ query: buildRollupQuery(window) });
  const url = `${deps.lokiUrl}/loki/api/v1/query?${params}`;
  // Cold-connect to Loki fails transiently (~1 in 3 — observed live), which would
  // otherwise 502 the board intermittently. Retry the connection a few times with
  // small backoff before giving up; only a genuinely-down Loki returns the refusal.
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
  const classes = parseRollupVector(await res.json());
  return { ok: true, window, total: classes.reduce((s, c) => s + c.count, 0), classes };
}
