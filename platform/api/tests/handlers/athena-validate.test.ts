/**
 * athena-validate handler — unit tests (#2180).
 *
 * SHACL-style integrity checker. Runs a fixed list of checks against the
 * chorus ontology in Fuseki; each check is a SELECT whose non-empty
 * bindings represent violations or warnings.
 *
 * Tests describe Jeff-visible behavior:
 *   - clean ontology → 200 + valid=true + empty arrays
 *   - one violation → 200 + valid=false + violations[] populated
 *   - one warning → 200 + valid=true + warnings[] populated (valid tracks violations only)
 *   - binding label used when present; falls back to stripped node URI
 *   - SPARQL throws → 500 + error envelope
 *   - duration_ms + checked count present in response body
 *
 * No SPARQL client, no Fuseki, no harness. Fakes pass canned results.
 */
import {
  fetchAthenaValidate,
  type AthenaValidateDeps,
  type SparqlBindingsResult,
} from '../../src/handlers/athena-validate';

function emptyResult(): SparqlBindingsResult {
  return { results: { bindings: [] } };
}

function binding(nodeUri: string, label?: string): SparqlBindingsResult {
  const b: { node: { value: string }; label?: { value: string } } = {
    node: { value: nodeUri },
  };
  if (label !== undefined) b.label = { value: label };
  return { results: { bindings: [b] } };
}

function deps(overrides: Partial<AthenaValidateDeps> = {}): AthenaValidateDeps {
  return {
    sparql: async () => emptyResult(),
    now: () => 1_000_000,
    timestamp: () => '2026-04-18T12:00:00-04:00',
    ...overrides,
  };
}

describe('fetchAthenaValidate (#2180)', () => {
  test('clean ontology returns 200 with valid=true and empty arrays', async () => {
    const r = await fetchAthenaValidate(deps());
    expect(r.status).toBe(200);
    const body = r.body as { valid: boolean; violations: unknown[]; warnings: unknown[]; checked: number };
    expect(body.valid).toBe(true);
    expect(body.violations).toEqual([]);
    expect(body.warnings).toEqual([]);
    expect(body.checked).toBeGreaterThan(0);
  });

  test('violation binding produces valid=false + violations[] entry', async () => {
    let callCount = 0;
    const r = await fetchAthenaValidate(deps({
      sparql: async () => {
        callCount++;
        return callCount === 1
          ? binding('https://jeffbridwell.com/chorus#orphan-product', 'Orphan Product')
          : emptyResult();
      },
    }));
    expect(r.status).toBe(200);
    const body = r.body as {
      valid: boolean;
      violations: Array<{ node: string; constraint: string; severity: string }>;
      warnings: unknown[];
    };
    expect(body.valid).toBe(false);
    expect(body.violations).toHaveLength(1);
    expect(body.violations[0].severity).toBe('violation');
    expect(body.violations[0].node).toBe('Orphan Product');
    expect(body.warnings).toEqual([]);
  });

  test('warning-severity binding lands in warnings[] and leaves valid=true', async () => {
    const r = await fetchAthenaValidate(deps({
      sparql: async (query) => {
        // Only the warning-severity check has "no instances" constraint wording
        if (query.includes('chorus:contains')) {
          return binding('https://jeffbridwell.com/chorus#empty-subdomain');
        }
        return emptyResult();
      },
    }));
    expect(r.status).toBe(200);
    const body = r.body as { valid: boolean; warnings: Array<{ node: string; severity: string }>; violations: unknown[] };
    expect(body.valid).toBe(true);
    expect(body.warnings).toHaveLength(1);
    expect(body.warnings[0].severity).toBe('warning');
    expect(body.violations).toEqual([]);
  });

  test('binding without label falls back to prefix-stripped node URI', async () => {
    let served = false;
    const r = await fetchAthenaValidate(deps({
      sparql: async () => {
        if (served) return emptyResult();
        served = true;
        return binding('https://jeffbridwell.com/chorus#bare-node');
      },
    }));
    const body = r.body as { violations: Array<{ node: string }> };
    expect(body.violations[0].node).toBe('bare-node');
  });

  test('SPARQL throws maps to 500 with error envelope', async () => {
    const r = await fetchAthenaValidate(deps({
      sparql: async () => { throw new Error('Fuseki down'); },
    }));
    expect(r.status).toBe(500);
    const body = r.body as { data: { error: string }; _meta: { error: boolean } };
    expect(body.data.error).toBe('Fuseki down');
    expect(body._meta.error).toBe(true);
  });

  test('non-Error throw stringifies in error message', async () => {
    const r = await fetchAthenaValidate(deps({
      sparql: async () => { throw 'network-timeout'; },
    }));
    expect(r.status).toBe(500);
    const body = r.body as { data: { error: string } };
    expect(body.data.error).toBe('network-timeout');
  });

  test('duration_ms recorded from now() delta', async () => {
    let n = 0;
    const r = await fetchAthenaValidate(deps({
      now: () => { n++; return n === 1 ? 1000 : 1042; },
    }));
    const body = r.body as { duration_ms: number };
    expect(body.duration_ms).toBe(42);
  });

  test('response includes timestamp from injected timestamp()', async () => {
    const r = await fetchAthenaValidate(deps({
      timestamp: () => 'FIXED-TS',
    }));
    const body = r.body as { timestamp: string };
    expect(body.timestamp).toBe('FIXED-TS');
  });

  test('checked count equals number of constraint checks run', async () => {
    const callTracker: number = 0;
    let seen = 0;
    const r = await fetchAthenaValidate(deps({
      sparql: async () => { seen++; return emptyResult(); },
    }));
    const body = r.body as { checked: number };
    expect(body.checked).toBe(seen);
    expect(seen).toBeGreaterThanOrEqual(6);
    void callTracker;
  });

  test('multiple bindings in one check produce multiple entries', async () => {
    let served = false;
    const r = await fetchAthenaValidate(deps({
      sparql: async () => {
        if (served) return emptyResult();
        served = true;
        return {
          results: {
            bindings: [
              { node: { value: 'https://jeffbridwell.com/chorus#a' }, label: { value: 'A' } },
              { node: { value: 'https://jeffbridwell.com/chorus#b' }, label: { value: 'B' } },
            ],
          },
        };
      },
    }));
    const body = r.body as { violations: Array<{ node: string }> };
    expect(body.violations.map((v) => v.node)).toEqual(['A', 'B']);
  });
});
