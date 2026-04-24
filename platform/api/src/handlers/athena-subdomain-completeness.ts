/**
 * GET /api/athena/subdomains/:id/completeness — Per-stage lifecycle gate (#2187).
 *
 * Returns which sections of the sub-domain are populated + whether the
 * create/wip/done lifecycle gates pass. Derivation:
 *   - 1 metadata query (ontology graph) + 11 per-predicate COUNT queries
 *     (instances graph), run in parallel to avoid TDB2 combinatorial blow-up
 *     on multi-OPTIONAL joins (#2175 rationale preserved).
 *   - 'sections' map: boolean per section (label/comment/owner/step/actors/…/edges)
 *   - 'lifecycle' stages: create/wip/done with required/met/missing/pass
 *
 * 404 when the meta query returns no bindings (sub-domain missing).
 */
import type { FetchResult } from './codebase-topology';

export interface SparqlMetaBinding {
  label?: { value: string };
  comment?: { value: string };
  ownerLabel?: { value: string };
  stepLabel?: { value: string };
  consumesCount?: { value: string };
  consumedByCount?: { value: string };
}

export interface SparqlMetaResult {
  results: { bindings: SparqlMetaBinding[] };
}

export interface SparqlCountResult {
  results: { bindings: Array<{ n?: { value: string } }> };
}

export interface AthenaCompletenessDeps {
  sparqlQuery: (query: string) => Promise<SparqlMetaResult | SparqlCountResult>;
  now?: () => number;
  envelope?: (name: string, data: unknown, durationMs: number, extra?: Record<string, unknown>) => unknown;
}

const CHORUS_PREFIX = 'https://jeffbridwell.com/chorus#';

const COUNT_PREDS = [
  ['actorCount', 'hasActor'],
  ['scenarioCount', 'hasScenario'],
  ['contractCount', 'hasContract'],
  ['priorArtCount', 'hasPriorArt'],
  ['pageCount', 'hasPage'],
  ['integrationCount', 'hasIntegration'],
  ['serviceCount', 'hasService'],
  ['persistenceCount', 'hasPersistence'],
  ['pipelineCount', 'hasPipeline'],
  ['logSourceCount', 'hasLogSource'],
  ['gapCount', 'hasGap'],
] as const;

function defaultEnvelope(name: string, data: unknown, durationMs: number, extra: Record<string, unknown> = {}) {
  return {
    _meta: { source: 'athena', query_name: name, duration_ms: durationMs, ...extra },
    data,
  };
}

function buildMetaQuery(sdUri: string): string {
  return `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> SELECT ?label ?comment ?ownerLabel ?stepLabel (COUNT(DISTINCT ?consumed) AS ?consumesCount) (COUNT(DISTINCT ?consumer) AS ?consumedByCount) WHERE { GRAPH <urn:chorus:ontology> { <${sdUri}> a chorus:SubDomain . OPTIONAL { <${sdUri}> rdfs:label ?label } OPTIONAL { <${sdUri}> rdfs:comment ?comment } OPTIONAL { <${sdUri}> chorus:ownedBy ?owner . ?owner rdfs:label ?ownerLabel } OPTIONAL { <${sdUri}> chorus:primaryStep ?step . ?step rdfs:label ?stepLabel } OPTIONAL { <${sdUri}> chorus:consumes ?consumed } OPTIONAL { ?consumer chorus:consumes <${sdUri}> } } } GROUP BY ?label ?comment ?ownerLabel ?stepLabel`;
}

function buildCountQuery(sdUri: string, predicate: string): string {
  return `PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT (COUNT(DISTINCT ?e) AS ?n) WHERE { GRAPH <urn:chorus:instances> { <${sdUri}> chorus:${predicate} ?e } }`;
}

function buildSections(b: SparqlMetaResult['results']['bindings'][number], counts: Record<string, number>): Record<string, boolean> {
  return {
    label: !!b.label,
    comment: !!b.comment,
    owner: !!b.ownerLabel,
    step: !!b.stepLabel,
    actors: counts.actorCount > 0,
    scenarios: counts.scenarioCount > 0,
    contract: counts.contractCount > 0,
    prior_art: counts.priorArtCount > 0,
    pages: counts.pageCount > 0,
    integrations: counts.integrationCount > 0,
    services: counts.serviceCount > 0,
    persistence: counts.persistenceCount > 0,
    pipeline: counts.pipelineCount > 0,
    logs: counts.logSourceCount > 0,
    gaps: counts.gapCount > 0,
    edges: (parseInt(b.consumesCount?.value ?? '0', 10) + parseInt(b.consumedByCount?.value ?? '0', 10)) > 0,
  };
}

function computeLifecycle(sections: Record<string, boolean>) {
  const lifecycle: Record<string, { required: string[]; met: string[]; missing: string[]; pass: boolean }> = {
    create: { required: ['label', 'owner', 'step', 'comment'], met: [], missing: [], pass: false },
    wip: { required: ['actors', 'edges'], met: [], missing: [], pass: false },
    done: { required: ['scenarios', 'contract'], met: [], missing: [], pass: false },
  };
  for (const gate of Object.values(lifecycle)) {
    gate.met = gate.required.filter((r) => sections[r]);
    gate.missing = gate.required.filter((r) => !sections[r]);
    gate.pass = gate.missing.length === 0;
  }
  return lifecycle;
}

export async function fetchAthenaSubdomainCompleteness(
  deps: AthenaCompletenessDeps,
  id: string,
): Promise<FetchResult> {
  const now = deps.now ?? Date.now;
  const envelope = deps.envelope ?? defaultEnvelope;
  const start = now();
  const sdUri = `${CHORUS_PREFIX}${id}`;

  try {
    const [metaRaw, ...countsRaw] = await Promise.all([
      deps.sparqlQuery(buildMetaQuery(sdUri)),
      ...COUNT_PREDS.map(([, pred]) => deps.sparqlQuery(buildCountQuery(sdUri, pred))),
    ]);
    const meta = metaRaw as SparqlMetaResult;
    if (meta.results.bindings.length === 0) {
      return {
        status: 404,
        body: envelope('subdomain-completeness', { error: `Sub-domain '${id}' not found` }, now() - start, { error: true }),
      };
    }
    const b = meta.results.bindings[0];
    const counts: Record<string, number> = {};
    COUNT_PREDS.forEach(([key], i) => {
      const cr = countsRaw[i] as SparqlCountResult;
      counts[key] = parseInt(cr.results.bindings[0]?.n?.value ?? '0', 10);
    });
    const sections = buildSections(b, counts);
    const lifecycle = computeLifecycle(sections);
    const present = Object.entries(sections).filter(([, v]) => v).map(([k]) => k);
    const missing = Object.entries(sections).filter(([, v]) => !v).map(([k]) => k);
    const percentage = Math.round((present.length / Object.keys(sections).length) * 100);
    return {
      status: 200,
      body: envelope(
        'subdomain-completeness',
        {
          subdomain: id,
          label: b.label?.value,
          step: b.stepLabel?.value,
          sections,
          present,
          missing,
          percentage,
          lifecycle,
        },
        now() - start,
        { count: present.length, total: Object.keys(sections).length },
      ),
    };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return {
      status: 500,
      body: envelope('subdomain-completeness', { error: message }, now() - start, { error: true }),
    };
  }
}
