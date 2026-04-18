/**
 * GET /api/athena/subdomains/:id/blast-radius — Who depends on this (#2187).
 */
import type { FetchResult } from './codebase-topology';

export interface SparqlConsumerBinding {
  consumer: { value: string };
  consumerLabel?: { value: string };
}

export interface SparqlBlastRadiusResult {
  results: { bindings: SparqlConsumerBinding[] };
}

export interface AthenaBlastRadiusDeps {
  sparql: (query: string) => Promise<SparqlBlastRadiusResult>;
  loadQuery: (name: string) => string;
  now?: () => number;
  envelope?: (name: string, data: unknown, durationMs: number, extra?: Record<string, unknown>) => unknown;
}

const CHORUS_PREFIX = 'https://jeffbridwell.com/chorus#';

function defaultEnvelope(name: string, data: unknown, durationMs: number, extra: Record<string, unknown> = {}) {
  return {
    _meta: { source: 'athena', query_name: name, duration_ms: durationMs, ...extra },
    data,
  };
}

function fallbackId(uri: string): string {
  const hashIdx = uri.lastIndexOf('#');
  return hashIdx === -1 ? uri : uri.slice(hashIdx + 1);
}

export async function fetchAthenaBlastRadius(
  deps: AthenaBlastRadiusDeps,
  id: string,
): Promise<FetchResult> {
  const now = deps.now ?? Date.now;
  const envelope = deps.envelope ?? defaultEnvelope;
  const start = now();
  const sdUri = `${CHORUS_PREFIX}${id}`;

  try {
    const query = deps.loadQuery('blast-radius').replace('$URI', sdUri);
    const result = await deps.sparql(query);
    const consumers = result.results.bindings.map((b) => ({
      uri: b.consumer.value,
      label: b.consumerLabel?.value ?? fallbackId(b.consumer.value),
    }));
    return {
      status: 200,
      body: envelope('blast-radius', { subdomain: id, consumers }, now() - start, { count: consumers.length }),
    };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return {
      status: 500,
      body: envelope('blast-radius', { error: message }, now() - start, { error: true }),
    };
  }
}
