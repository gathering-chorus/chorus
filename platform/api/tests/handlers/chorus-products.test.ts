/**
 * chorus-products handler — unit tests (#2189).
 *
 * GET /api/chorus/products returns product hierarchy:
 *   products[] → subproducts[] → domains[]
 *   plus direct product→domain edges for products without intermediate subproducts
 *
 * Tests verify:
 *   - empty results → 200 + empty products + elapsed_ms present
 *   - nested: product → subproduct → domain renders correctly
 *   - subproduct carries owner from ownerLabel
 *   - direct product→domain edges land in product.domains, not a subproduct
 *   - duplicate domain under same subproduct is deduped
 *   - sparql throws → 500 with error
 */
import {
  fetchChorusProducts,
  type ChorusProductsDeps,
  type SparqlFn,
  type SparqlResult,
} from '../../src/handlers/chorus-products';

function mkResult(bindings: Record<string, string>[]): SparqlResult {
  return {
    results: {
      bindings: bindings.map((row) => {
        const out: Record<string, { value: string }> = {};
        for (const [k, v] of Object.entries(row)) out[k] = { value: v };
        return out;
      }),
    },
  };
}

function sparqlQueue(responses: SparqlResult[]): SparqlFn {
  let i = 0;
  return async () => {
    if (i >= responses.length) throw new Error(`unexpected sparql call ${i}`);
    return responses[i++];
  };
}

function deps(overrides: Partial<ChorusProductsDeps>): ChorusProductsDeps {
  return {
    sparql: sparqlQueue([mkResult([]), mkResult([])]),
    now: () => 1000,
    ...overrides,
  };
}

describe('fetchChorusProducts (#2189 /api/chorus/products)', () => {
  test('empty results → 200 with empty products array and elapsed_ms', async () => {
    let t = 1000;
    const r = await fetchChorusProducts(deps({
      sparql: sparqlQueue([mkResult([]), mkResult([])]),
      now: () => (t += 5),
    }));
    expect(r.status).toBe(200);
    const b = r.body as { products: unknown[]; elapsed_ms: number };
    expect(b.products).toEqual([]);
    expect(b.elapsed_ms).toBe(5);
  });

  test('product → subproduct → domain renders with owner', async () => {
    const r = await fetchChorusProducts(deps({
      sparql: sparqlQueue([
        mkResult([
          {
            productLabel: 'Gathering',
            spLabel: 'Harvest',
            sdLabel: 'Music',
            ownerLabel: 'Kade',
          },
        ]),
        mkResult([]),
      ]),
    }));
    const b = r.body as { products: Array<{ label: string; subproducts: Array<{ label: string; owner: string | null; domains: string[] }>; domains: string[] }> };
    expect(b.products).toHaveLength(1);
    expect(b.products[0].label).toBe('Gathering');
    expect(b.products[0].subproducts).toEqual([
      { label: 'Harvest', owner: 'Kade', domains: ['Music'] },
    ]);
    expect(b.products[0].domains).toEqual([]);
  });

  test('subproduct without ownerLabel gets owner=null', async () => {
    const r = await fetchChorusProducts(deps({
      sparql: sparqlQueue([
        mkResult([{ productLabel: 'P', spLabel: 'SP', sdLabel: 'D' }]),
        mkResult([]),
      ]),
    }));
    const b = r.body as { products: Array<{ subproducts: Array<{ owner: string | null }> }> };
    expect(b.products[0].subproducts[0].owner).toBeNull();
  });

  test('direct product→domain edges land in product.domains', async () => {
    const r = await fetchChorusProducts(deps({
      sparql: sparqlQueue([
        mkResult([]),
        mkResult([
          { productLabel: 'Borg', domainLabel: 'Scheduling' },
          { productLabel: 'Borg', domainLabel: 'Ingestion' },
        ]),
      ]),
    }));
    const b = r.body as { products: Array<{ label: string; subproducts: unknown[]; domains: string[] }> };
    expect(b.products).toHaveLength(1);
    expect(b.products[0].label).toBe('Borg');
    expect(b.products[0].subproducts).toEqual([]);
    expect(b.products[0].domains).toEqual(['Scheduling', 'Ingestion']);
  });

  test('duplicate domain under same subproduct is deduped', async () => {
    const r = await fetchChorusProducts(deps({
      sparql: sparqlQueue([
        mkResult([
          { productLabel: 'P', spLabel: 'SP', sdLabel: 'Music' },
          { productLabel: 'P', spLabel: 'SP', sdLabel: 'Music' }, // duplicate
          { productLabel: 'P', spLabel: 'SP', sdLabel: 'Photos' },
        ]),
        mkResult([]),
      ]),
    }));
    const b = r.body as { products: Array<{ subproducts: Array<{ domains: string[] }> }> };
    expect(b.products[0].subproducts[0].domains).toEqual(['Music', 'Photos']);
  });

  test('sparql throws → 500 with error message', async () => {
    const r = await fetchChorusProducts({
      sparql: async () => { throw new Error('fuseki down'); },
      now: () => 1000,
    });
    expect(r.status).toBe(500);
    expect(r.body).toEqual({ error: 'fuseki down' });
  });
});
