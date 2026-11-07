// @test-type: unit — real SPARQL (in-process oxigraph) over a TTL fixture, no Fuseki.
// @domain: domains
/**
 * #4738 — blast radius walks the whole dependency closure, not one hop.
 *
 * The fixture is the live chain into `infrastructure`: Pulse's `spine` consumes
 * `security`, which dependsOn `infrastructure`. Changing infrastructure hits
 * Pulse, two hops out, across two predicates. The 1-hop, consumes-only walk
 * returned `pipelines` and nothing else.
 */
import * as path from 'path';
import { fetchDomainBlastRadius, blastClosure, type DomainFacetDeps } from '../../src/handlers/domain-facets';
import { makeSparqlFromTtlGraphs } from '../fixtures/oxigraph-sparql';

const FIXTURE = path.join(__dirname, '../fixtures/blast-radius-4738.ttl');

interface Ring { ring: number; weight: number; domains: string[] }
interface BlastData {
  domain: string;
  edges: Array<{ target: string; relationship: string; direction: string; hops?: number }>;
  rings: Ring[];
  score: number;
  paths: Record<string, string[]>;
}

function deps(): DomainFacetDeps {
  const sparql = makeSparqlFromTtlGraphs(FIXTURE, ['urn:chorus:domains:domains', 'urn:chorus:ontology']);
  return {
    sparql: sparql as unknown as DomainFacetDeps['sparql'],
    envelope: (name: string, data: unknown, _ms: number, extra: Record<string, unknown> = {}) => ({ _meta: { name, ...extra }, data }),
    now: () => 1000,
  };
}

const dataOf = (r: { body: unknown }): BlastData => (r.body as { data: BlastData }).data;

describe('#4738 transitive blast radius', () => {
  test('NEGATIVE PROOF: Pulse (spine) is 2 hops from infrastructure and must be in its blast radius', async () => {
    const data = dataOf(await fetchDomainBlastRadius(deps(), 'infrastructure'));
    const targets = data.edges.map((e) => e.target);
    // The 1-hop consumes-only walk returned ['pipelines'] here: spine, messages,
    // identity and the three dependsOn neighbours were all invisible.
    expect(targets).toEqual(expect.arrayContaining(['spine', 'messages', 'identity', 'security', 'time', 'logs', 'pipelines']));
  });

  test('a mixed consumes → dependsOn chain is followed, with its full path', async () => {
    const data = dataOf(await fetchDomainBlastRadius(deps(), 'infrastructure'));
    expect(data.paths.spine).toEqual(['spine', 'security', 'infrastructure']);
    expect(data.edges.find((e) => e.target === 'spine')?.hops).toBe(2);
  });

  test('rings are hop distance, weighted 1, 1, 2, 3, 5, 8', async () => {
    const data = dataOf(await fetchDomainBlastRadius(deps(), 'infrastructure'));
    expect(data.rings).toEqual([
      { ring: 1, weight: 1, domains: ['logs', 'pipelines', 'security', 'time'] },
      { ring: 2, weight: 1, domains: ['identity', 'messages', 'spine'] },
    ]);
    expect(data.score).toBe(4 * 1 + 3 * 1);
  });

  test('each domain sits in its nearest ring only', async () => {
    const data = dataOf(await fetchDomainBlastRadius(deps(), 'infrastructure'));
    const all = data.rings.flatMap((r) => r.domains);
    expect(all.length).toBe(new Set(all).size);
  });

  test('a leaf domain has an empty radius and a zero score', async () => {
    const data = dataOf(await fetchDomainBlastRadius(deps(), 'spine'));
    expect(data.rings).toEqual([]);
    expect(data.score).toBe(0);
  });
});

describe('#4738 ring weights beyond 2 hops', () => {
  test('a 7-hop chain fills rings 1..6 with weights 1,1,2,3,5,8 and caps hop 7 at ring 6', () => {
    const chain = ['d1', 'd2', 'd3', 'd4', 'd5', 'd6', 'd7'];
    const edges = chain.map((d, i) => ({ from: d, rel: i % 2 ? 'dependsOn' : 'consumes', to: i === 0 ? 'root' : chain[i - 1] }));
    const c = blastClosure('root', edges);
    expect(c.rings).toEqual([
      { ring: 1, weight: 1, domains: ['d1'] },
      { ring: 2, weight: 1, domains: ['d2'] },
      { ring: 3, weight: 2, domains: ['d3'] },
      { ring: 4, weight: 3, domains: ['d4'] },
      { ring: 5, weight: 5, domains: ['d5'] },
      { ring: 6, weight: 8, domains: ['d6', 'd7'] },
    ]);
    expect(c.score).toBe(1 + 1 + 2 + 3 + 5 + 8 + 8);
    expect(c.hops.get('d7')).toBe(7);
    expect(c.paths.d7).toEqual([...chain].reverse().concat('root'));
  });

  test('a cycle back to the root does not loop or count the root', () => {
    const c = blastClosure('root', [
      { from: 'a', rel: 'consumes', to: 'root' },
      { from: 'root', rel: 'consumes', to: 'a' },
    ]);
    expect(c.rings).toEqual([{ ring: 1, weight: 1, domains: ['a'] }]);
  });
});
