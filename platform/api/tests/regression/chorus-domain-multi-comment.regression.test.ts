/**
 * Regression: chorus-domain drops reads/writes when service has multiple rdfs:comment triples (#2212).
 *
 * Log evidence (2026-04-20): live /api/chorus/domain/chorus showed "Pulse" 3x,
 * "chorus-api" 2x, "Clearing" 2x in services.items — one row per rdfs:comment.
 * All 13 rows had description but zero had reads/writes. 12 edges verified
 * present in urn:chorus:instances via direct SPARQL.
 *
 * Root cause: GROUP BY ?e ?label ?comment creates N rows per service with N
 * comments. LIMIT 20 slices across these duplicate rows; reads/writes bindings
 * land on some rows but those rows fall past the LIMIT window.
 *
 * Fix: SAMPLE(?comment) in SELECT, remove ?comment from GROUP BY.
 */

import {
  fetchChorusDomain,
  type ChorusDomainDeps,
} from '../../src/handlers/chorus-domain';

const SERVICE_URI = 'https://jeffbridwell.com/chorus#pulse-service';

function makeDeps(sparql: ChorusDomainDeps['sparql']): ChorusDomainDeps {
  return {
    domainRegistry: { chorus: { label: 'Chorus', description: 'Team coordination', product: 'chorus', step: 'directing' } },
    getCards: () => [],
    readDomainHtml: () => null,
    // Return a non-null completeness so subdomainId is set and buildSparqlSections fires
    fetchCompleteness: async () => ({ percentage: 50, present: ['label'], missing: [] }),
    sparql,
  };
}

describe('fetchChorusDomain — multi-comment service regression (#2212)', () => {
  test('post-fix SPARQL returns one row per service — reads intact in handler output', async () => {
    // After the fix (SAMPLE(?comment), GROUP BY ?e ?label), the database collapses
    // multi-comment services to one row. This test verifies the handler correctly
    // surfaces reads/writes from that single post-fix row.
    const sparqlMock = jest.fn().mockImplementation(async (q: string) => {
      if (q.includes('hasService')) {
        return {
          results: {
            bindings: [
              // Post-fix: one row per entity, reads/writes intact
              {
                e: { value: SERVICE_URI },
                label: { value: 'Pulse' },
                comment: { value: 'seed description' }, // SAMPLE picked one comment
                owners: { value: '' },
                reads: { value: 'SQLite||Fuseki' },
                writes: { value: '' },
                consumes: { value: '' },
              },
            ],
          },
        };
      }
      return { results: { bindings: [] } };
    });

    const r = await fetchChorusDomain(makeDeps(sparqlMock), 'chorus');
    expect(r.status).toBe(200);
    const body = r.body as any;
    const serviceDetails = body.sections?.services?.itemDetails ?? [];
    const pulseRows = serviceDetails.filter((s: any) => s.label === 'Pulse');

    expect(pulseRows.length).toBe(1);
    expect(pulseRows[0].reads).toBeDefined();
    expect(pulseRows[0].reads).toEqual(['SQLite', 'Fuseki']);
  });

  test('service with one comment continues to show reads and writes', async () => {
    const sparqlMock = jest.fn().mockImplementation(async (q: string) => {
      if (q.includes('hasService')) {
        return {
          results: {
            bindings: [{
              e: { value: SERVICE_URI },
              label: { value: 'Pulse' },
              comment: { value: 'single description' },
              owners: { value: '' },
              reads: { value: 'SQLite' },
              writes: { value: 'spine-log' },
              consumes: { value: '' },
            }],
          },
        };
      }
      return { results: { bindings: [] } };
    });

    const r = await fetchChorusDomain(makeDeps(sparqlMock), 'chorus');
    expect(r.status).toBe(200);
    const body = r.body as any;
    const serviceDetails = body.sections?.services?.itemDetails ?? [];
    const pulse = serviceDetails.find((s: any) => s.label === 'Pulse');
    expect(pulse).toBeDefined();
    expect(pulse.reads).toEqual(['SQLite']);
    expect(pulse.writes).toEqual(['spine-log']);
  });
});
