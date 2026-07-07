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
 * canned bindings; no live Fuseki, no owl-api.
 */
import {
  projectSecuredSurfaces,
  type EmitDeps,
  type SparqlRows,
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