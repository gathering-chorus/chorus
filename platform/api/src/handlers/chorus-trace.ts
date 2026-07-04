/**
 * GET /api/chorus/trace/:correlationId + /api/chorus/trace/integrations/:domain
 * (extracted #2189).
 *
 * /trace/:correlationId → hops for a given correlation id, ordered by hop,
 *   PLUS the spine events carrying that id as trace_id (#3621). The hop table
 *   was the only source before, which made the viewer read {hops:[]} while
 *   Loki held the run's real events (Jeff's #3609 ask) — the werk pipeline
 *   emits spine events, not HTTP hops. The fold merges both into one
 *   chronological `timeline`; hops remain for back-compat.
 * /trace/integrations/:domain → observed (src,dest,call_stack) pairs w/ frequency
 *
 * Both accept a pre-opened db handle (table init handled by the adapter).
 * The spine fetcher is injected (server wires it to logsForTrace's Loki path)
 * so tests run hermetically and a Loki outage degrades to hops-only — the
 * trace view must never 500 because one of its two sources is down.
 */
import type Database from 'better-sqlite3';

export interface SpineEventRow {
  ts?: string;
  event?: string;
  [key: string]: unknown;
}

export interface TraceDeps {
  db: Database.Database;
  /** #3621 — fetch spine events by trace_id (Loki-backed in prod). Optional:
   *  absent → legacy hops-only behavior. */
  fetchSpineEvents?: (traceId: string) => Promise<SpineEventRow[]>;
}

export interface TraceCorrelationResult {
  status: number;
  body: {
    correlationId: string;
    hops: unknown[];
    events: SpineEventRow[];
    timeline: Array<{ kind: 'hop' | 'spine'; ts?: string; [key: string]: unknown }>;
    spine_error?: string;
  };
}

export interface TraceIntegrationsResult {
  status: number;
  body: { domain: string; integrations: unknown[] };
}

export async function fetchTraceByCorrelation(
  correlationId: string,
  { db, fetchSpineEvents }: TraceDeps,
): Promise<TraceCorrelationResult> {
  const hops = db
    .prepare('SELECT * FROM traces WHERE correlation_id = ? ORDER BY hop ASC')
    .all(correlationId);

  let events: SpineEventRow[] = [];
  let spine_error: string | undefined;
  if (fetchSpineEvents) {
    try {
      events = await fetchSpineEvents(correlationId);
    } catch (err) {
      // Degrade, don't die: hops still render; the gap is named, not silent.
      spine_error = err instanceof Error ? err.message : String(err);
      events = [];
    }
  }

  const spineSorted = [...events].sort((a, b) => String(a.ts ?? '').localeCompare(String(b.ts ?? '')));
  const timeline: TraceCorrelationResult['body']['timeline'] = [
    ...(hops as Array<Record<string, unknown>>).map((h) => ({ kind: 'hop' as const, ...h })),
    ...spineSorted.map((e) => ({ kind: 'spine' as const, ...e })),
  ].sort((a, b) => String((a as { ts?: string }).ts ?? '').localeCompare(String((b as { ts?: string }).ts ?? '')));

  const body: TraceCorrelationResult['body'] = { correlationId, hops, events: spineSorted, timeline };
  if (spine_error !== undefined) body.spine_error = spine_error;
  return { status: 200, body };
}

export function fetchTraceIntegrations(
  domain: string,
  { db }: TraceDeps,
): TraceIntegrationsResult {
  const integrations = db
    .prepare(
      `SELECT source_service, dest_service, call_stack, COUNT(*) as frequency
       FROM traces
       WHERE source_domain = ? AND dest_service IS NOT NULL
       GROUP BY source_service, dest_service, call_stack
       ORDER BY frequency DESC`,
    )
    .all(domain);
  return { status: 200, body: { domain, integrations } };
}
