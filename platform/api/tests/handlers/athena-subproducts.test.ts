/**
 * athena-subproducts handler — unit tests (#2187).
 *
 * Lists sub-products with denormalized owner label, domain count, and
 * consumes count. Each binding becomes a structured record.
 *
 * Real logic exercised:
 *   - binding.sp.value → uri
 *   - binding.label.value → label (fallback: URI fragment after #)
 *   - binding.ownerLabel.value → owner (default: null)
 *   - binding.domainCount.value → int (default: 0)
 *   - binding.consumesCount.value → int (default: 0)
 *   - count metadata = bindings length
 *   - SPARQL throw → 500 + error envelope
 */
import {
  fetchAthenaSubproducts,
  type AthenaSubproductsDeps,
  type SparqlSubproductBinding,
} from '../../src/handlers/athena-subproducts';

function result(bindings: SparqlSubproductBinding[]) {
  return { results: { bindings } };
}

function deps(overrides: Partial<AthenaSubproductsDeps> = {}): AthenaSubproductsDeps {
  return {
    sparql: async () => result([]),
    loadQuery: (name: string) => `# query: ${name}`,
    now: () => 1_000_000,
    ...overrides,
  };
}

describe('fetchAthenaSubproducts (#2187)', () => {
  test('empty result returns 200 with empty array and count=0', async () => {
    const r = await fetchAthenaSubproducts(deps());
    expect(r.status).toBe(200);
    const body = r.body as { data: Array<unknown>; _meta: { count: number } };
    expect(body.data).toEqual([]);
    expect(body._meta.count).toBe(0);
  });

  test('fully populated binding maps all five fields', async () => {
    const r = await fetchAthenaSubproducts(deps({
      sparql: async () => result([{
        sp: { value: 'https://jeffbridwell.com/chorus#pulse' },
        label: { value: 'Pulse' },
        ownerLabel: { value: 'Silas' },
        domainCount: { value: '3' },
        consumesCount: { value: '5' },
      }]),
    }));
    const body = r.body as { data: Array<{ uri: string; label: string; owner: string; domainCount: number; consumesCount: number }> };
    expect(body.data[0]).toEqual({
      uri: 'https://jeffbridwell.com/chorus#pulse',
      label: 'Pulse',
      owner: 'Silas',
      domainCount: 3,
      consumesCount: 5,
    });
  });

  test('missing label falls back to URI fragment after #', async () => {
    const r = await fetchAthenaSubproducts(deps({
      sparql: async () => result([{
        sp: { value: 'https://jeffbridwell.com/chorus#bare-sp' },
      }]),
    }));
    const body = r.body as { data: Array<{ label: string }> };
    expect(body.data[0].label).toBe('bare-sp');
  });

  test('missing owner defaults to null', async () => {
    const r = await fetchAthenaSubproducts(deps({
      sparql: async () => result([{
        sp: { value: 'https://jeffbridwell.com/chorus#x' },
        label: { value: 'X' },
      }]),
    }));
    const body = r.body as { data: Array<{ owner: string | null }> };
    expect(body.data[0].owner).toBeNull();
  });

  test('missing domainCount defaults to 0', async () => {
    const r = await fetchAthenaSubproducts(deps({
      sparql: async () => result([{
        sp: { value: 'https://jeffbridwell.com/chorus#x' },
        label: { value: 'X' },
      }]),
    }));
    const body = r.body as { data: Array<{ domainCount: number }> };
    expect(body.data[0].domainCount).toBe(0);
  });

  test('missing consumesCount defaults to 0', async () => {
    const r = await fetchAthenaSubproducts(deps({
      sparql: async () => result([{
        sp: { value: 'https://jeffbridwell.com/chorus#x' },
        label: { value: 'X' },
      }]),
    }));
    const body = r.body as { data: Array<{ consumesCount: number }> };
    expect(body.data[0].consumesCount).toBe(0);
  });

  test('count metadata reflects bindings length', async () => {
    const r = await fetchAthenaSubproducts(deps({
      sparql: async () => result([
        { sp: { value: '#a' }, label: { value: 'A' } },
        { sp: { value: '#b' }, label: { value: 'B' } },
      ]),
    }));
    const body = r.body as { _meta: { count: number } };
    expect(body._meta.count).toBe(2);
  });

  test('SPARQL throws returns 500 with error envelope', async () => {
    const r = await fetchAthenaSubproducts(deps({
      sparql: async () => { throw new Error('pods/sparql 503'); },
    }));
    expect(r.status).toBe(500);
    const body = r.body as { data: { error: string }; _meta: { error: boolean } };
    expect(body.data.error).toBe('pods/sparql 503');
    expect(body._meta.error).toBe(true);
  });

  test('loadQuery is called with name "subproducts"', async () => {
    let seenName = '';
    await fetchAthenaSubproducts(deps({
      loadQuery: (name) => { seenName = name; return '# q'; },
    }));
    expect(seenName).toBe('subproducts');
  });
});
