/* eslint-disable security/detect-object-injection -- #3429: object indexing is over internally-derived keys (Loki field names from typed config), not untrusted input */
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
  let query = '{job=~".+"} |= "\\"level\\":\\"error\\""';
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
