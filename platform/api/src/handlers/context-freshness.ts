/**
 * GET /api/chorus/context/freshness (#2252 migration).
 *
 * Wraps the existing /api/chorus/freshness payload in the common Context
 * envelope. The inner `fetchFreshness` handler stays unchanged; this is a
 * pure envelope-adaptation layer so `/api/chorus/freshness` can return a 301
 * redirect for one wave, then retire.
 *
 * Scope: domain (chorus) — freshness is a chorus-product observability
 * surface. Envelope carries step + product + domain.
 */

import {
  stampHeader,
  buildEnvelope,
  type StampSparqlClient,
  type ContextEnvelope,
} from '../lib/context-envelope';

export interface ContextFreshnessDeps {
  sparql: StampSparqlClient;
  /** Runs the underlying /api/chorus/freshness handler. Returns its shape. */
  runFreshness: () => { status: number; body: unknown };
}

export interface ContextFreshnessResponse {
  status: number;
  body: ContextEnvelope<unknown> | { error: string };
}

export async function fetchContextFreshness(
  deps: ContextFreshnessDeps,
  sourceUrl: string,
): Promise<ContextFreshnessResponse> {
  const inner = deps.runFreshness();
  if (inner.status !== 200) {
    return { status: inner.status, body: inner.body as { error: string } };
  }
  const header = await stampHeader(deps.sparql, 'chorus');
  return {
    status: 200,
    body: buildEnvelope(header, sourceUrl, inner.body),
  };
}
