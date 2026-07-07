/**
 * #3618 — the surface-table emitter: "secured" is a model fact, projected.
 *
 * Reads APISurface instances that carry a securedBy edge from the model and
 * projects each into a SecuredSurface row for the envelope (security-envelope.ts).
 * This is the generation step of the #3414 seam extended to chorus-api: no
 * hand-maintained gate list — the graph's securedBy edges ARE the list, and
 * this projects them. Committed output is drift-checked against the graph.
 *
 * Boot contract: an empty model yields an empty table (the envelope then gates
 * nothing — mixed-state by construction). A SPARQL error THROWS: boot must fail
 * loud, never silently degrade to an ungated API.
 */
import type { SecuredSurface } from './security-envelope';

export interface SparqlRows {
  results: { bindings: Array<Record<string, { value: string }>> };
}

export interface EmitDeps {
  sparql: (query: string) => Promise<SparqlRows>;
}

const NS = 'https://jeffbridwell.com/chorus#';

const SURFACE_QUERY = `PREFIX chorus: <${NS}>
SELECT ?surface ?method ?pathPrefix ?requiresScope WHERE {
  GRAPH <urn:chorus:ontology> {
    ?surface a chorus:APISurface ;
             chorus:securedBy ?gate .
    OPTIONAL { ?surface chorus:httpMethod ?method }
    OPTIONAL { ?surface chorus:pathPrefix ?pathPrefix }
    OPTIONAL { ?surface chorus:requiresScope ?requiresScope }
  }
}`;

function slug(iri: string): string {
  return iri.startsWith(NS) ? iri.slice(NS.length) : iri;
}

/** Project the model's secured APISurface instances into envelope rows. */
export async function projectSecuredSurfaces(deps: EmitDeps): Promise<SecuredSurface[]> {
  const res = await deps.sparql(SURFACE_QUERY);
  const out: SecuredSurface[] = [];
  for (const b of res.results.bindings) {
    const surface = b.surface?.value;
    const method = b.method?.value;
    const pathPrefix = b.pathPrefix?.value;
    // A surface with no method or path prefix can't be gated — skip rather than
    // emit a half-formed row that would match nothing (or everything).
    if (!surface || !method || !pathPrefix) continue;
    out.push({
      method,
      pathPrefix,
      requiresScope: b.requiresScope?.value ?? '',
      surface: slug(surface),
    });
  }
  return out;
}

export { SURFACE_QUERY };
