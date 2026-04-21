/**
 * GET /api/chorus/context/quality/summary (#2252 migration).
 *
 * Envelope-wraps the existing /api/chorus/quality/summary handler output.
 * Legacy path stays live; 301 + telemetry wave follows once callers are
 * carded.
 */

import {
  stampHeader,
  buildEnvelope,
  type StampSparqlClient,
  type ContextEnvelope,
} from '../lib/context-envelope';

export interface ContextQualitySummaryDeps {
  sparql: StampSparqlClient;
  runQuality: () => Promise<{ status: number; body: unknown }>;
}

export interface ContextQualitySummaryResponse {
  status: number;
  body: ContextEnvelope<unknown> | { error: string };
}

export async function fetchContextQualitySummary(
  deps: ContextQualitySummaryDeps,
  sourceUrl: string,
): Promise<ContextQualitySummaryResponse> {
  const inner = await deps.runQuality();
  if (inner.status !== 200) {
    return { status: inner.status, body: inner.body as { error: string } };
  }
  const header = await stampHeader(deps.sparql, 'chorus');
  return { status: 200, body: buildEnvelope(header, sourceUrl, inner.body) };
}
