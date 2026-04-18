/**
 * GET /api/athena/steps — Value-stream steps with sub-domains at each (#2187).
 *
 * SPARQL returns one row per (step, sub-domain) pair. Handler groups rows
 * into one record per step URI and accumulates sub-domains into a list.
 * Steps with no sub-domain still produce a record (empty subdomains[],
 * domainCount=0).
 */
import type { FetchResult } from './codebase-topology';

export interface SparqlStepBinding {
  step: { value: string };
  stepLabel?: { value: string };
  sd?: { value: string };
  sdLabel?: { value: string };
  sdOwnerLabel?: { value: string };
}

export interface SparqlStepsResult {
  results: { bindings: SparqlStepBinding[] };
}

export interface AthenaStepsDeps {
  sparql: (query: string) => Promise<SparqlStepsResult>;
  loadQuery: (name: string) => string;
  now?: () => number;
  envelope?: (name: string, data: unknown, durationMs: number, extra?: Record<string, unknown>) => unknown;
}

interface SubdomainEntry {
  uri: string;
  label: string;
  owner: string | null;
}

interface StepEntry {
  uri: string;
  label: string;
  domainCount: number;
  subdomains: SubdomainEntry[];
}

function defaultEnvelope(name: string, data: unknown, durationMs: number, extra: Record<string, unknown> = {}) {
  return {
    _meta: { source: 'athena', query_name: name, duration_ms: durationMs, ...extra },
    data,
  };
}

function fallbackLabel(uri: string): string {
  const hashIdx = uri.lastIndexOf('#');
  return hashIdx === -1 ? uri : uri.slice(hashIdx + 1);
}

export async function fetchAthenaSteps(deps: AthenaStepsDeps): Promise<FetchResult> {
  const now = deps.now ?? Date.now;
  const envelope = deps.envelope ?? defaultEnvelope;
  const start = now();

  try {
    const result = await deps.sparql(deps.loadQuery('steps'));
    const stepMap = new Map<string, StepEntry>();
    for (const b of result.results.bindings) {
      const key = b.step.value;
      if (!stepMap.has(key)) {
        stepMap.set(key, {
          uri: key,
          label: b.stepLabel?.value ?? fallbackLabel(key),
          domainCount: 0,
          subdomains: [],
        });
      }
      if (b.sd) {
        const entry = stepMap.get(key)!;
        entry.subdomains.push({
          uri: b.sd.value,
          label: b.sdLabel?.value ?? fallbackLabel(b.sd.value),
          owner: b.sdOwnerLabel?.value ?? null,
        });
        entry.domainCount = entry.subdomains.length;
      }
    }
    const steps = Array.from(stepMap.values());
    return {
      status: 200,
      body: envelope('steps', steps, now() - start, { count: steps.length }),
    };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return {
      status: 500,
      body: envelope('steps', { error: message }, now() - start, { error: true }),
    };
  }
}
