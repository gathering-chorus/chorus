/**
 * athena-products handler — unit tests (#2187).
 *
 * Lists all chorus:Product instances from the ontology graph. Each binding
 * becomes { uri, label }. Label falls back to the URI fragment after '#'
 * when rdfs:label is missing.
 *
 * Real logic exercised:
 *   - binding.product.value → uri
 *   - binding.label.value → label (when present)
 *   - product.value.split('#').pop() → label (fallback, missing label)
 *   - count metadata reflects bindings length
 *   - SPARQL throw → 500 + error envelope
 */
import {
  fetchAthenaProducts,
  type AthenaProductsDeps,
  type SparqlProductBinding,
} from '../../src/handlers/athena-products';

function result(bindings: SparqlProductBinding[]) {
  return { results: { bindings } };
}

function deps(overrides: Partial<AthenaProductsDeps> = {}): AthenaProductsDeps {
  return {
    sparql: async () => result([]),
    loadQuery: (name: string) => `# query: ${name}`,
    now: () => 1_000_000,
    ...overrides,
  };
}

describe('fetchAthenaProducts (#2187)', () => {
  test('empty ontology returns 200 with empty products array and count=0', async () => {
    const r = await fetchAthenaProducts(deps());
    expect(r.status).toBe(200);
    const body = r.body as { data: Array<unknown>; _meta: { count: number } };
    expect(body.data).toEqual([]);
    expect(body._meta.count).toBe(0);
  });

  test('binding with rdfs:label uses the label verbatim', async () => {
    const r = await fetchAthenaProducts(deps({
      sparql: async () => result([
        { product: { value: 'https://jeffbridwell.com/chorus#gathering' }, label: { value: 'Gathering' } },
      ]),
    }));
    const body = r.body as { data: Array<{ uri: string; label: string }> };
    expect(body.data).toEqual([
      { uri: 'https://jeffbridwell.com/chorus#gathering', label: 'Gathering' },
    ]);
  });

  test('binding missing label falls back to URI fragment after #', async () => {
    const r = await fetchAthenaProducts(deps({
      sparql: async () => result([
        { product: { value: 'https://jeffbridwell.com/chorus#bare-product' } },
      ]),
    }));
    const body = r.body as { data: Array<{ uri: string; label: string }> };
    expect(body.data[0].label).toBe('bare-product');
  });

  test('binding missing label and no # in URI falls back to the URI itself', async () => {
    const r = await fetchAthenaProducts(deps({
      sparql: async () => result([
        { product: { value: 'urn:chorus:unnamed' } },
      ]),
    }));
    const body = r.body as { data: Array<{ label: string }> };
    expect(body.data[0].label).toBe('urn:chorus:unnamed');
  });

  test('count metadata equals the number of products', async () => {
    const r = await fetchAthenaProducts(deps({
      sparql: async () => result([
        { product: { value: 'https://jeffbridwell.com/chorus#a' }, label: { value: 'A' } },
        { product: { value: 'https://jeffbridwell.com/chorus#b' }, label: { value: 'B' } },
        { product: { value: 'https://jeffbridwell.com/chorus#c' }, label: { value: 'C' } },
      ]),
    }));
    const body = r.body as { _meta: { count: number }; data: Array<unknown> };
    expect(body._meta.count).toBe(3);
    expect(body.data).toHaveLength(3);
  });

  test('SPARQL throws returns 500 with error envelope', async () => {
    const r = await fetchAthenaProducts(deps({
      sparql: async () => { throw new Error('Fuseki connection refused'); },
    }));
    expect(r.status).toBe(500);
    const body = r.body as { data: { error: string }; _meta: { error: boolean } };
    expect(body.data.error).toBe('Fuseki connection refused');
    expect(body._meta.error).toBe(true);
  });

  test('loadQuery is called with name "products"', async () => {
    let seenName = '';
    await fetchAthenaProducts(deps({
      loadQuery: (name) => { seenName = name; return '# q'; },
    }));
    expect(seenName).toBe('products');
  });

  test('loaded query string is passed through to sparql()', async () => {
    let seenQuery = '';
    await fetchAthenaProducts(deps({
      loadQuery: () => 'SELECT ?product ?label WHERE { ?product a chorus:Product }',
      sparql: async (q) => { seenQuery = q; return result([]); },
    }));
    expect(seenQuery).toBe('SELECT ?product ?label WHERE { ?product a chorus:Product }');
  });
});
