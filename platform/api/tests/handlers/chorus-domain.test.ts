/**
 * chorus-domain handler — unit tests (#2198).
 *
 * Pure handler over 5 injected deps. Each test seeds fakes and asserts
 * real behavior: 404 on unknown domain, card filter, HTML section parse,
 * SPARQL section fallback, ownership walk-up direct vs inherited,
 * completeness fallback, ICD flag.
 */
import {
  fetchChorusDomain,
  parseDomainHtml,
  type ChorusDomainDeps,
  type DomainBoardCard,
  type Completeness,
} from '../../src/handlers/chorus-domain';

const REG = {
  photos: { product: 'gathering', step: 'harvesting', description: 'Three eras.' },
  chorus: { product: 'chorus', step: 'building', description: 'Coordination.' },
  books: { product: 'gathering', step: 'harvesting', description: 'Reading.' },
};

function deps(over: Partial<ChorusDomainDeps> = {}): ChorusDomainDeps {
  return {
    domainRegistry: REG,
    getCards: () => [],
    readDomainHtml: () => null,
    fetchCompleteness: async () => null,
    sparql: async () => ({ results: { bindings: [] } }),
    ...over,
  };
}

function card(over: Partial<DomainBoardCard> = {}): DomainBoardCard {
  return {
    id: '1', title: 'T', status: 'Next', owner: 'wren', type: 'enhance', tags: 'domain:photos',
    ...over,
  };
}

function binding(obj: Record<string, string>) {
  const out: Record<string, { value: string }> = {};
  for (const [k, v] of Object.entries(obj)) out[k] = { value: v };
  return out;
}

describe('parseDomainHtml', () => {
  test('parses h2 + table + li into section map', () => {
    const html = `
      <h2>Scenarios</h2>
      <table><tr><td>upload</td><td>user uploads photo</td></tr><tr><td>search</td><td>find photos</td></tr></table>
      <h2>Edge Cases</h2>
      <ul><li>no wifi</li><li>storage full</li></ul>
    `;
    const s = parseDomainHtml(html);
    expect(Object.keys(s)).toEqual(['scenarios', 'edge_cases']);
    expect(s.scenarios.table).toEqual([['upload', 'user uploads photo'], ['search', 'find photos']]);
    expect(s.edge_cases.items).toEqual(['no wifi', 'storage full']);
  });

  test('section with only table has no items key', () => {
    const html = '<h2>Refs</h2><table><tr><td>a</td></tr></table>';
    const s = parseDomainHtml(html);
    expect(s.refs.items).toBeUndefined();
    expect(s.refs.table).toEqual([['a']]);
  });

  test('section with only list has no table key', () => {
    const html = '<h2>Risks</h2><ul><li>crashes</li></ul>';
    const s = parseDomainHtml(html);
    expect(s.risks.table).toBeUndefined();
    expect(s.risks.items).toEqual(['crashes']);
  });

  test('empty html returns empty sections', () => {
    expect(parseDomainHtml('')).toEqual({});
  });

  test('strips nested tags from cells and items', () => {
    const html = '<h2>X</h2><ul><li><a href="#">link</a></li></ul><table><tr><td><b>bold</b></td></tr></table>';
    const s = parseDomainHtml(html);
    expect(s.x.items).toEqual(['link']);
    expect(s.x.table).toEqual([['bold']]);
  });
});

describe('fetchChorusDomain — validation', () => {
  test('unknown domain → 404 with validDomains list', async () => {
    const r = await fetchChorusDomain(deps(), 'mystery');
    expect(r.status).toBe(404);
    const body = r.body as { error: string; validDomains: string[] };
    expect(body.error).toMatch(/unknown domain: mystery/i);
    expect(body.validDomains).toContain('photos');
  });

  test('domain name lowercased before lookup', async () => {
    const r = await fetchChorusDomain(deps(), 'PHOTOS');
    expect(r.status).toBe(200);
    const body = r.body as { domain: string };
    expect(body.domain).toBe('photos');
  });

  test('registered domain returns 200 with meta fields', async () => {
    const body = (await fetchChorusDomain(deps(), 'photos')).body as {
      domain: string; product: string; step: string; description: string;
    };
    expect(body.product).toBe('gathering');
    expect(body.step).toBe('harvesting');
    expect(body.description).toBe('Three eras.');
  });
});

describe('fetchChorusDomain — cards', () => {
  test('filters Done and Won\'t Do', async () => {
    const cards: DomainBoardCard[] = [
      card({ id: '1', status: 'WIP' }),
      card({ id: '2', status: 'Done' }),
      card({ id: '3', status: "Won't Do" }),
      card({ id: '4', status: 'Next' }),
    ];
    const body = (await fetchChorusDomain(deps({ getCards: () => cards }), 'photos')).body as {
      cards: { total: number; items: Array<{ id: string }> };
    };
    expect(body.cards.total).toBe(2);
    expect(body.cards.items.map((c) => c.id)).toEqual(['1', '4']);
  });

  test('filters to matching domain tag', async () => {
    const cards: DomainBoardCard[] = [
      card({ id: '1', tags: 'domain:photos' }),
      card({ id: '2', tags: 'domain:music' }),
    ];
    const body = (await fetchChorusDomain(deps({ getCards: () => cards }), 'photos')).body as {
      cards: { total: number };
    };
    expect(body.cards.total).toBe(1);
  });

  test('wip and blocked counts', async () => {
    const cards: DomainBoardCard[] = [
      card({ id: '1', status: 'WIP' }),
      card({ id: '2', status: 'WIP' }),
      card({ id: '3', status: 'Blocked' }),
      card({ id: '4', status: 'Next' }),
    ];
    const body = (await fetchChorusDomain(deps({ getCards: () => cards }), 'photos')).body as {
      cards: { wip: number; blocked: number; total: number };
    };
    expect(body.cards.wip).toBe(2);
    expect(body.cards.blocked).toBe(1);
    expect(body.cards.total).toBe(4);
  });
});

describe('fetchChorusDomain — sections (HTML path)', () => {
  test('HTML present populates sections, SPARQL not called', async () => {
    let sparqlCalls = 0;
    const sparql = async () => { sparqlCalls++; return { results: { bindings: [] } }; };
    const html = '<h2>Scenarios</h2><ul><li>upload</li></ul>';
    const body = (await fetchChorusDomain(
      deps({ readDomainHtml: () => html, sparql }),
      'photos',
    )).body as { sections: Record<string, { items?: string[] }> };
    expect(body.sections.scenarios?.items).toEqual(['upload']);
    expect(sparqlCalls).toBe(0);
  });

  test('readDomainHtml throw is swallowed', async () => {
    const body = (await fetchChorusDomain(
      deps({ readDomainHtml: () => { throw new Error('fs gone'); } }),
      'photos',
    )).body as { sections: Record<string, unknown> };
    expect(body.sections).toEqual({});
  });
});

describe('fetchChorusDomain — completeness fallback', () => {
  test('-service resolves first', async () => {
    const calls: string[] = [];
    const fetchCompleteness = async (id: string): Promise<Completeness | null> => {
      calls.push(id);
      if (id === 'photos-service') {
        return { percentage: 75, present: ['a'], missing: ['b'], lifecycle: { wip: { pass: true } } };
      }
      return null;
    };
    const body = (await fetchChorusDomain(deps({ fetchCompleteness }), 'photos')).body as {
      completeness: { percentage: number } | null;
    };
    expect(calls).toEqual(['photos-service']);
    expect(body.completeness?.percentage).toBe(75);
  });

  test('-service null → -domain fallback', async () => {
    const calls: string[] = [];
    const fetchCompleteness = async (id: string): Promise<Completeness | null> => {
      calls.push(id);
      if (id === 'photos-domain') {
        return { percentage: 40, present: [], missing: [], lifecycle: {} };
      }
      return null;
    };
    const body = (await fetchChorusDomain(deps({ fetchCompleteness }), 'photos')).body as {
      completeness: { percentage: number } | null;
    };
    expect(calls).toEqual(['photos-service', 'photos-domain']);
    expect(body.completeness?.percentage).toBe(40);
  });

  test('both null → completeness null in response', async () => {
    const body = (await fetchChorusDomain(deps(), 'photos')).body as {
      completeness: unknown | null;
    };
    expect(body.completeness).toBeNull();
  });

  test('completeness throw swallowed', async () => {
    const body = (await fetchChorusDomain(
      deps({ fetchCompleteness: async () => { throw new Error('api down'); } }),
      'photos',
    )).body as { completeness: unknown | null };
    expect(body.completeness).toBeNull();
  });
});

describe('fetchChorusDomain — SPARQL section fallback', () => {
  test('fires only when HTML empty AND subdomainId resolved', async () => {
    let sparqlCalls = 0;
    const sparql = async () => { sparqlCalls++; return { results: { bindings: [] } }; };
    // No HTML, no completeness → no SPARQL calls (subdomainId null)
    await fetchChorusDomain(deps({ sparql }), 'photos');
    expect(sparqlCalls).toBe(0);

    // No HTML + completeness resolves → SPARQL fires
    sparqlCalls = 0;
    const fetchCompleteness = async () => ({ percentage: 50, present: [], missing: [], lifecycle: {} } as Completeness);
    await fetchChorusDomain(deps({ sparql, fetchCompleteness }), 'photos');
    expect(sparqlCalls).toBeGreaterThan(0);
  });

  test('direct owner preferred', async () => {
    const sparql = async (query: string) => {
      if (query.includes('hasScenario')) {
        return {
          results: {
            bindings: [binding({ label: 'Upload flow', owners: 'Kade', reads: 'photos-db' })],
          },
        };
      }
      return { results: { bindings: [] } };
    };
    const fetchCompleteness = async () => ({ percentage: 50, present: [], missing: [], lifecycle: {} } as Completeness);
    const body = (await fetchChorusDomain(deps({ sparql, fetchCompleteness }), 'photos')).body as {
      sections: Record<string, { items?: string[]; itemDetails?: Array<Record<string, unknown>> }>;
    };
    expect(body.sections.scenarios.items).toEqual(['Upload flow']);
    const detail = body.sections.scenarios.itemDetails![0];
    expect(detail.owner).toBe('Kade');
    expect(detail.ownerInherited).toBeUndefined();
    expect(detail.reads).toEqual(['photos-db']);
  });

  test('inherited owner when direct missing', async () => {
    const sparql = async (query: string) => {
      if (query.includes('SELECT ?ownerLabel')) {
        // parent owner query
        return { results: { bindings: [binding({ ownerLabel: 'Wren' })] } };
      }
      if (query.includes('hasScenario')) {
        return {
          results: {
            bindings: [binding({ label: 'Orphan scenario' })],
          },
        };
      }
      return { results: { bindings: [] } };
    };
    const fetchCompleteness = async () => ({ percentage: 50, present: [], missing: [], lifecycle: {} } as Completeness);
    const body = (await fetchChorusDomain(deps({ sparql, fetchCompleteness }), 'photos')).body as {
      sections: Record<string, { itemDetails?: Array<Record<string, unknown>> }>;
    };
    const detail = body.sections.scenarios.itemDetails![0];
    expect(detail.owner).toBe('Wren');
    expect(detail.ownerInherited).toBe(true);
  });

  test('multiple direct owners returned as array', async () => {
    const sparql = async (query: string) => {
      if (query.includes('hasService')) {
        return {
          results: {
            bindings: [binding({ label: 'Shared svc', owners: 'Kade||Silas' })],
          },
        };
      }
      return { results: { bindings: [] } };
    };
    const fetchCompleteness = async () => ({ percentage: 50, present: [], missing: [], lifecycle: {} } as Completeness);
    const body = (await fetchChorusDomain(deps({ sparql, fetchCompleteness }), 'photos')).body as {
      sections: Record<string, { itemDetails?: Array<Record<string, unknown>> }>;
    };
    expect(body.sections.services.itemDetails![0].owner).toEqual(['Kade', 'Silas']);
  });

  test('reads/writes/consumes split on ||', async () => {
    const sparql = async (query: string) => {
      if (query.includes('hasScenario')) {
        return {
          results: {
            bindings: [binding({
              label: 'Complex',
              owners: 'Kade',
              reads: 'a||b',
              writes: 'c',
              consumes: 'd||e||f',
            })],
          },
        };
      }
      return { results: { bindings: [] } };
    };
    const fetchCompleteness = async () => ({ percentage: 50, present: [], missing: [], lifecycle: {} } as Completeness);
    const body = (await fetchChorusDomain(deps({ sparql, fetchCompleteness }), 'photos')).body as {
      sections: Record<string, { itemDetails?: Array<Record<string, unknown>> }>;
    };
    const detail = body.sections.scenarios.itemDetails![0];
    expect(detail.reads).toEqual(['a', 'b']);
    expect(detail.writes).toEqual(['c']);
    expect(detail.consumes).toEqual(['d', 'e', 'f']);
  });

  test('bindings without label are skipped', async () => {
    const sparql = async (query: string) => {
      if (query.includes('hasActor')) {
        return {
          results: {
            bindings: [binding({ owners: 'Wren' }), binding({ label: 'RealActor' })],
          },
        };
      }
      return { results: { bindings: [] } };
    };
    const fetchCompleteness = async () => ({ percentage: 50, present: [], missing: [], lifecycle: {} } as Completeness);
    const body = (await fetchChorusDomain(deps({ sparql, fetchCompleteness }), 'photos')).body as {
      sections: Record<string, { items?: string[] }>;
    };
    expect(body.sections.actors.items).toEqual(['RealActor']);
  });

  test('SPARQL throw swallowed — sections stay empty', async () => {
    const fetchCompleteness = async () => ({ percentage: 50, present: [], missing: [], lifecycle: {} } as Completeness);
    const body = (await fetchChorusDomain(
      deps({ sparql: async () => { throw new Error('fuseki down'); }, fetchCompleteness }),
      'photos',
    )).body as { sections: Record<string, unknown> };
    expect(body.sections).toEqual({});
  });
});

describe('fetchChorusDomain — ICD flag', () => {
  test('photos is ICD', async () => {
    const body = (await fetchChorusDomain(deps(), 'photos')).body as { hasIcd: boolean; icdEndpoint: string };
    expect(body.hasIcd).toBe(true);
    expect(body.icdEndpoint).toBe('/api/icd/domains/photos');
  });

  test('books is not ICD', async () => {
    const body = (await fetchChorusDomain(deps(), 'books')).body as { hasIcd: boolean };
    expect(body.hasIcd).toBe(false);
  });

  test('chorus is not ICD', async () => {
    const body = (await fetchChorusDomain(deps(), 'chorus')).body as { hasIcd: boolean };
    expect(body.hasIcd).toBe(false);
  });
});
