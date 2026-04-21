/**
 * GET /api/athena/subdomains/:id — Single sub-domain detail (#2187).
 *
 * Resolves the sub-domain's owner, step, comment, and three relationship
 * lists (consumers, things it consumes, child domains), plus its contained
 * instances. Empty result set → 404 with pointer to the list endpoint.
 *
 * Dedup semantics for the three relationship lists: one entry per URI.
 * Instances are deduped by URI in a Map, preserving first-seen metadata.
 */
import type { FetchResult } from './codebase-topology';

export interface SparqlDetailBinding {
  label?: { value: string };
  ownerLabel?: { value: string };
  stepLabel?: { value: string };
  comment?: { value: string };
  consumer?: { value: string };
  consumerLabel?: { value: string };
  consumed?: { value: string };
  consumedLabel?: { value: string };
  child?: { value: string };
  childLabel?: { value: string };
  instance?: { value: string };
  instanceLabel?: { value: string };
  instanceComment?: { value: string };
  instanceType?: { value: string };
  instanceTypeLabel?: { value: string };
}

export interface SparqlDetailResult {
  results: { bindings: SparqlDetailBinding[] };
}

export interface AthenaSubdomainDetailDeps {
  sparql: (query: string) => Promise<SparqlDetailResult>;
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

interface UriLabel {
  uri: string;
  label: string;
}

interface ChildDomain {
  uri: string;
  id: string;
  label: string;
}

interface Instance {
  uri: string;
  id: string;
  label: string;
  comment: string | null;
  type: string | null;
}

function dedupByUri<T extends { uri: string }>(entries: T[]): T[] {
  const seen = new Set<string>();
  const out: T[] = [];
  for (const e of entries) {
    if (seen.has(e.uri)) continue;
    seen.add(e.uri);
    out.push(e);
  }
  return out;
}

type Binding = Awaited<ReturnType<AthenaSubdomainDetailDeps['sparql']>>['results']['bindings'][number];

function extractConsumers(bindings: Binding[]): UriLabel[] {
  return dedupByUri(
    bindings
      .filter((b) => b.consumer)
      .map((b) => ({ uri: b.consumer!.value, label: b.consumerLabel?.value ?? fallbackId(b.consumer!.value) })),
  );
}

function extractConsumes(bindings: Binding[]): UriLabel[] {
  return dedupByUri(
    bindings
      .filter((b) => b.consumed)
      .map((b) => ({ uri: b.consumed!.value, label: b.consumedLabel?.value ?? fallbackId(b.consumed!.value) })),
  );
}

function extractChildDomains(bindings: Binding[]): ChildDomain[] {
  return dedupByUri(
    bindings
      .filter((b) => b.child)
      .map((b) => ({
        uri: b.child!.value,
        id: fallbackId(b.child!.value),
        label: b.childLabel?.value ?? fallbackId(b.child!.value),
      })),
  );
}

function extractInstances(bindings: Binding[]): Instance[] {
  const instanceMap = new Map<string, Instance>();
  for (const b of bindings) {
    if (!b.instance) continue;
    const uri = b.instance.value;
    if (instanceMap.has(uri)) continue;
    instanceMap.set(uri, {
      uri,
      id: fallbackId(uri),
      label: b.instanceLabel?.value ?? fallbackId(uri),
      comment: b.instanceComment?.value ?? null,
      type: b.instanceTypeLabel?.value ?? (b.instanceType?.value ? fallbackId(b.instanceType.value) : null),
    });
  }
  return Array.from(instanceMap.values());
}

export async function fetchAthenaSubdomainDetail(
  deps: AthenaSubdomainDetailDeps,
  id: string,
): Promise<FetchResult> {
  const now = deps.now ?? Date.now;
  const envelope = deps.envelope ?? defaultEnvelope;
  const start = now();
  const sdUri = `${CHORUS_PREFIX}${id}`;

  try {
    const query = deps.loadQuery('subdomain-detail').split('$URI').join(sdUri);
    const result = await deps.sparql(query);
    const bindings = result.results.bindings;

    if (bindings.length === 0) {
      return {
        status: 404,
        body: envelope(
          'subdomain-detail',
          {
            error: `Sub-domain '${id}' not found`,
            suggestion: 'Use GET /api/athena/subdomains to list all available sub-domains.',
          },
          now() - start,
          { error: true },
        ),
      };
    }

    const first = bindings[0];
    return {
      status: 200,
      body: envelope(
        'subdomain-detail',
        {
          uri: sdUri,
          id,
          label: first.label?.value ?? id,
          owner: first.ownerLabel?.value ?? null,
          step: first.stepLabel?.value ?? null,
          comment: first.comment?.value ?? null,
          consumedBy: extractConsumers(bindings),
          consumes: extractConsumes(bindings),
          domains: extractChildDomains(bindings),
          instances: extractInstances(bindings),
        },
        now() - start,
      ),
    };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return {
      status: 500,
      body: envelope('subdomain-detail', { error: message }, now() - start, { error: true }),
    };
  }
}
