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
  source?: string;
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

function parseLokiLine(line: string): LogRow | null {
  try {
    const obj = JSON.parse(line) as Record<string, unknown>;
    const { ts, event, role, level, card_id, trace_id, source, ...rest } = obj;
    return {
      ts: typeof ts === 'string' ? ts : new Date().toISOString(),
      event: typeof event === 'string' ? event : undefined,
      role: typeof role === 'string' ? role : undefined,
      level: typeof level === 'string' ? level : undefined,
      card_id: typeof card_id === 'number' ? card_id : undefined,
      trace_id: typeof trace_id === 'string' ? trace_id : undefined,
      source: typeof source === 'string' ? source : undefined,
      payload: rest,
    };
  } catch {
    return null;
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
    for (const [, line] of stream.values ?? []) {
      const row = parseLokiLine(line);
      if (row) events.push(row);
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
  const query = `{job=~".+"} |~ "${args.trace_id}"`;
  const range = resolveTimeRange(deps, undefined, undefined, window);
  if ('error' in range) return { ok: false, reason: 'time-range-invalid', detail: range.error };
  return executeLokiQuery(query, range.startNs, range.endNs, 1000, deps);
}
