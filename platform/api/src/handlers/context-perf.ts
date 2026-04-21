/**
 * GET /api/chorus/context/perf (#2252 migration).
 *
 * Envelope-wraps the existing /api/chorus/perf handler output. Legacy path
 * stays live; 301 + telemetry wave follows once callers are carded.
 */

import {
  stampHeader,
  buildEnvelope,
  type StampSparqlClient,
  type ContextEnvelope,
} from '../lib/context-envelope';

export interface ContextPerfDeps {
  sparql: StampSparqlClient;
  runPerf: () => Promise<{ status: number; body: unknown }>;
}

export interface ContextPerfResponse {
  status: number;
  body: ContextEnvelope<unknown> | { error: string };
}

export async function fetchContextPerf(
  deps: ContextPerfDeps,
  sourceUrl: string,
): Promise<ContextPerfResponse> {
  const inner = await deps.runPerf();
  if (inner.status !== 200) {
    return { status: inner.status, body: inner.body as { error: string } };
  }
  const header = await stampHeader(deps.sparql, 'chorus');
  return { status: 200, body: buildEnvelope(header, sourceUrl, inner.body) };
}
