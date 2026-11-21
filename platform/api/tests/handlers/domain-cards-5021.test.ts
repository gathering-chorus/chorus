// @test-type: unit — signal:security is fixture-data (the bad-name case is a crafted SPARQL string, refused before any query)
// @domain: cards
// @card: #5021
/**
 * #5021 — the Cards fold on /athena/domain.html reads /api/chorus/domain/:name/cards: the
 * board's domain:<name> cards as a list, for any domain. NEGATIVE: a card of another domain, or of
 * a name that only starts the same, is not listed; a bad name is refused.
 */
import { fetchDomainCards, type BoardCard } from '../../src/handlers/domain-cards';
import { fetchDomainDecisions, type DomainFacetDeps } from '../../src/handlers/domain-facets';
import type { SparqlResult } from '../../src/handlers/athena-health';

const envelope = (name: string, data: unknown, _ms: number, extra: Record<string, unknown> = {}) => ({ _meta: { name, ...extra }, data });
const emptyResult = (): SparqlResult => ({ results: { bindings: [] } });
const deps = (overrides: Partial<DomainFacetDeps> = {}): DomainFacetDeps => ({ sparql: async () => emptyResult(), envelope, now: () => 1000, ...overrides });

const card = (id: string, status: string, tags: string): BoardCard => ({ id, title: `card ${id}`, status, owner: 'Kade', priority: 'P2', tags });
const board = [
  // tags exactly as the board cache holds them: `cards list`'s bracket, pipe-separated
  card('5017', 'Done', 'Kade|P2|domain:principles|type:enhance'),
  card('5021', 'WIP', 'Kade|P2|domain:principles|type:fix|sequence:athena'),
  card('5020', 'Done', 'Kade|P2|domain:chorus|type:enhance'),
  card('4006', 'Later', 'Wren|P3|domain:principles-old|type:new'),
  card('4999', 'Next', 'Silas|P2|type:fix|domain:principles'),
];

test('lists the domain\'s cards, open first, then newest first', () => {
  const r = fetchDomainCards(() => board, 'principles');
  expect(r.status).toBe(200);
  const body = r.body as { cards: Array<{ id: string; status: string }> };
  expect(body.cards.map((c) => c.id)).toEqual(['5021', '4999', '5017']);
  expect(Object.keys(body.cards[0]).sort()).toEqual(['id', 'owner', 'priority', 'status', 'title']);
});

test('NEGATIVE: another domain\'s card and a lookalike label are not listed', () => {
  const body = fetchDomainCards(() => board, 'principles').body as { cards: Array<{ id: string }> };
  expect(body.cards.map((c) => c.id)).not.toContain('5020');
  expect(body.cards.map((c) => c.id)).not.toContain('4006');
});

test('NEGATIVE: a name a label cannot carry is a 400', () => {
  expect(fetchDomainCards(() => board, 'princ.*').status).toBe(400);
});

test('any domain, not a fixed list: a domain no card names answers an empty list, not a 404', () => {
  const r = fetchDomainCards(() => board, 'values');
  expect(r.status).toBe(200);
  expect((r.body as { cards: unknown[] }).cards).toEqual([]);
});

// --- the Decisions fold (AC 7, Wren 18:23): the decisions domain's ADR and Approval rows ---

describe('#5021 Decisions fold', () => {
  // #5021 — the fold reads the decisions domain's ADR and Approval rows, joined by hasDomain
  test('shapes ADR and Approval rows from the decisions domain graph', async () => {
    const C = 'https://jeffbridwell.com/chorus#';
    const result: SparqlResult = {
      results: {
        bindings: [
          { s: { value: `${C}adr-051` }, t: { value: `${C}ADR` }, title: { value: 'ADR-051 domains graphs' }, status: { value: 'accepted' } },
          { s: { value: `${C}approval-17` }, t: { value: `${C}Approval` }, title: { value: 'a yes' }, date: { value: '2026-11-18' } },
        ],
      },
    };
    let seen = '';
    const d = deps({ sparql: async (q) => { seen = q; return result; } });
    const r = await fetchDomainDecisions(d, 'principles');
    expect(r.status).toBe(200);
    const body = r.body as { data: { decisions: Array<{ id: string; type: string; title: string }> } };
    expect(body.data.decisions.map((x) => [x.id, x.type])).toEqual([['adr-051', 'ADR'], ['approval-17', 'Approval']]);
    expect(seen).toContain('GRAPH <urn:chorus:domains:decisions>');
    expect(seen).toContain('<https://jeffbridwell.com/chorus#principles>');
  });

  test('NEGATIVE: it never reads the retired urn:chorus:decisions graph or chorus:Decision', async () => {
    let seen = '';
    const d = deps({ sparql: async (q) => { seen = q; return emptyResult(); } });
    await fetchDomainDecisions(d, 'principles');
    expect(seen).not.toContain('<urn:chorus:decisions>');
    expect(seen).not.toContain('chorus:Decision ');
  });

  test('NEGATIVE: a failed query is a 502, never an empty fold', async () => {
    const d = deps({ sparql: async () => { throw new Error('oops'); } });
    const r = await fetchDomainDecisions(d, 'awareness');
    expect(r.status).toBe(502);
  });
  test('NEGATIVE: a name that is not a domain name is a 400 and no query runs', async () => {
    let asked = false;
    const d = deps({ sparql: async () => { asked = true; return emptyResult(); } });
    const r = await fetchDomainDecisions(d, 'x> } DROP ALL #');
    expect(r.status).toBe(400);
    expect(asked).toBe(false);
  });

  test('one row per decision: the optional fields are grouped, never multiplied', async () => {
    let seen = '';
    const d = deps({ sparql: async (q) => { seen = q; return emptyResult(); } });
    await fetchDomainDecisions(d, 'principles');
    expect(seen).toMatch(/GROUP BY \?s \?t/);
    expect(seen).toContain('SAMPLE(?ti) AS ?title');
  });
});
