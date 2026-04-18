/**
 * GET /api/chorus/trace/:correlationId + /api/chorus/trace/integrations/:domain
 * (extracted #2189).
 *
 * /trace/:correlationId → hops for a given correlation id, ordered by hop
 * /trace/integrations/:domain → observed (src,dest,call_stack) pairs w/ frequency
 *
 * Both accept a pre-opened db handle (table init handled by the adapter).
 */
import type Database from 'better-sqlite3';

export interface TraceDeps {
  db: Database.Database;
}

export interface TraceCorrelationResult {
  status: number;
  body: { correlationId: string; hops: unknown[] };
}

export interface TraceIntegrationsResult {
  status: number;
  body: { domain: string; integrations: unknown[] };
}

export function fetchTraceByCorrelation(
  correlationId: string,
  { db }: TraceDeps,
): TraceCorrelationResult {
  const hops = db
    .prepare('SELECT * FROM traces WHERE correlation_id = ? ORDER BY hop ASC')
    .all(correlationId);
  return { status: 200, body: { correlationId, hops } };
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
