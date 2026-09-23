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
  // #3619 — a binding row OMITS keys bound only by OPTIONAL clauses (method,
  // pathPrefix, requiresScope in SURFACE_QUERY), so a value can be absent. The
  // `| undefined` makes that honest — the b.x?.value guards below are then
  // load-bearing, not redundant (no-unnecessary-condition, Kade #3618 lint).
  results: { bindings: Array<Record<string, { value: string } | undefined>> };
}

export interface EmitDeps {
  sparql: (query: string) => Promise<SparqlRows>;
}

const NS = 'https://jeffbridwell.com/chorus#';

// #4273 — THE GRAPH THIS NAMES IS THE WHOLE GATE. The 29 APISurface rows
// carrying securedBy live in <urn:chorus:domains:security> (a row's home is
// its own domain graph); this query pinned <urn:chorus:ontology>, matched
// nothing, and chorus-api logged `security.envelope.loaded surfaces=0` on
// every boot while CHORUS_SECURITY_ENVELOPE_ENABLE=1 — the gate switched on
// and holding nothing. An unauthenticated POST wrote a row into the live
// principles graph on 2026-09-21 through that hole.
//
// Pinned, not `GRAPH ?g`: a fixture graph that happened to carry an
// APISurface would otherwise mount or move a live gate.
const SURFACE_GRAPH = 'urn:chorus:domains:security';

const SURFACE_QUERY = `PREFIX chorus: <${NS}>
SELECT ?surface ?method ?pathPrefix ?requiresScope WHERE {
  GRAPH <${SURFACE_GRAPH}> {
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

export { SURFACE_QUERY, SURFACE_GRAPH };
