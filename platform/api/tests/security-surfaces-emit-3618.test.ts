// @test-type: unit
/**
 * #3618 — the surface-table emitter (hermetic tier).
 *
 * projectSecuredSurfaces(deps) queries the model for APISurface instances that
 * carry a securedBy edge, and projects each into the SecuredSurface rows the
 * envelope loads at boot. This is the generation step: "secured" is a model
 * fact (securedBy edge), the table is its projection — not hand-maintained.
 *
 * Brings its own world (#3528): the SPARQL client is injected and returns
 * canned bindings; no live Fuseki, no athena-make.
 */
import {
  projectSecuredSurfaces,
  type EmitDeps,
  type SparqlRows,
  SURFACE_QUERY,
} from '../src/security-surfaces-emit';

function rows(...bindings: Record<string, string>[]): SparqlRows {
  return { results: { bindings: bindings.map((b) => {
    const o: Record<string, { value: string }> = {};
    for (const k of Object.keys(b)) o[k] = { value: b[k] };
    return o;
  }) } };
}

function deps(over: Partial<EmitDeps> = {}): EmitDeps {
  return {
    sparql: async () => rows(),
    ...over,
  };
}

describe('projectSecuredSurfaces (#3618)', () => {
  test('projects a secured surface into an envelope row', async () => {
    const table = await projectSecuredSurfaces(deps({
      sparql: async () => rows({
        surface: 'https://jeffbridwell.com/chorus#surface-index-writes',
        method: 'POST',
        pathPrefix: '/api/chorus/reindex',
        requiresScope: 'urn:chorus:index',
      }),
    }));
    expect(table).toHaveLength(1);
    expect(table[0]).toEqual({
      method: 'POST',
      pathPrefix: '/api/chorus/reindex',
      requiresScope: 'urn:chorus:index',
      surface: 'surface-index-writes', // IRI stripped to slug for the spine field
    });
  });

  test('projects multiple surfaces preserving each scope', async () => {
    const table = await projectSecuredSurfaces(deps({
      sparql: async () => rows(
        { surface: 'https://jeffbridwell.com/chorus#surface-index-writes', method: 'POST', pathPrefix: '/api/chorus/reindex', requiresScope: 'urn:chorus:index' },
        { surface: 'https://jeffbridwell.com/chorus#surface-discover-writes', method: 'POST', pathPrefix: '/api/athena/discover-', requiresScope: 'urn:chorus:domains:code' },
      ),
    }));
    expect(table).toHaveLength(2);
    expect(table.map((r) => r.surface).sort()).toEqual(['surface-discover-writes', 'surface-index-writes']);
    expect(table.find((r) => r.surface === 'surface-discover-writes')?.requiresScope).toBe('urn:chorus:domains:code');
  });

  test('empty model → empty table (mixed-state boot safety: gates nothing)', async () => {
    const table = await projectSecuredSurfaces(deps());
    expect(table).toEqual([]);
  });

  test('a surface missing method or pathPrefix is skipped, not emitted half-formed', async () => {
    const table = await projectSecuredSurfaces(deps({
      sparql: async () => rows(
        { surface: 'https://jeffbridwell.com/chorus#surface-ok', method: 'POST', pathPrefix: '/api/x', requiresScope: 'urn:chorus:x' },
        { surface: 'https://jeffbridwell.com/chorus#surface-broken', requiresScope: 'urn:chorus:y' }, // no method/path
      ),
    }));
    expect(table).toHaveLength(1);
    expect(table[0].surface).toBe('surface-ok');
  });

  test('a surface with no requiresScope defaults to empty scope (envelope will refuse it 403)', async () => {
    const table = await projectSecuredSurfaces(deps({
      sparql: async () => rows({
        surface: 'https://jeffbridwell.com/chorus#surface-noscopde', method: 'POST', pathPrefix: '/api/z',
      }),
    }));
    expect(table).toHaveLength(1);
    expect(table[0].requiresScope).toBe('');
  });

  test('sparql failure throws (boot fails loud, never silently gates nothing on error)', async () => {
    await expect(projectSecuredSurfaces(deps({
      sparql: async () => { throw new Error('fuseki down'); },
    }))).rejects.toThrow('fuseki down');
  });
});
/**
 * #4273 — the loader looked in the wrong graph, so the gate loaded ZERO
 * surfaces on every boot while CHORUS_SECURITY_ENVELOPE_ENABLE=1.
 *
 * Measured 2026-09-22: 29 APISurface rows carrying securedBy live in
 * <urn:chorus:domains:security> (a row's home is its own domain graph), and
 * SURFACE_QUERY pinned <urn:chorus:ontology> — zero matches, gate open, an
 * unauthenticated POST wrote a row into the live principles graph on 09-21.
 *
 * The fake below is a tiny two-graph store rather than canned bindings: canned
 * bindings answer whatever they are handed and so cannot tell a query that
 * finds the rows from one that does not — the exact distinction this card is
 * about.
 */
describe('#4273 — the surface query finds rows where they actually live', () => {
  // Answers only the rows in the graph the query names; a query pinning a
  // graph that holds nothing gets nothing back, like the real store.
  const twoGraphStore = (q: string): SparqlRows => {
    const row = {
      surface: { value: 'https://jeffbridwell.com/chorus#surface-principles-post' },
      method: { value: 'POST' },
      pathPrefix: { value: '/api/athena/subdomains/loom-principles/principles' },
      requiresScope: { value: 'urn:chorus:scope:write' },
    };
    const pinsOntology = q.includes('GRAPH <urn:chorus:ontology>');
    const pinsSecurity = q.includes('GRAPH <urn:chorus:domains:security>');
    // The live store: the rows are in the security domain graph, nowhere else.
    if (pinsOntology) return { results: { bindings: [] } };
    if (pinsSecurity) return { results: { bindings: [row] } };
    return { results: { bindings: [row] } }; // unpinned (GRAPH ?g) sees every graph
  };

  test('surfaces load from the security domain graph, not the ontology graph', async () => {
    const table = await projectSecuredSurfaces({
      sparql: async (q: string) => twoGraphStore(q),
    });
    expect(table).toHaveLength(1);
    expect(table[0].pathPrefix).toBe('/api/athena/subdomains/loom-principles/principles');
  });

  // NEGATIVE PROOF (#3734): the check above must be able to go RED. Run the
  // SAME store against the OLD query text and confirm it yields nothing — the
  // state the gate was actually in all day.
  test('the retired ontology-pinned query returns zero against the same store', () => {
    const oldQuery = 'GRAPH <urn:chorus:ontology> { ?surface a chorus:APISurface }';
    expect(twoGraphStore(oldQuery).results.bindings).toHaveLength(0);
    expect(twoGraphStore(SURFACE_QUERY).results.bindings.length).toBeGreaterThan(0);
  });

  test('SURFACE_QUERY does not pin the ontology graph', () => {
    expect(SURFACE_QUERY).not.toContain('urn:chorus:ontology');
  });
});
