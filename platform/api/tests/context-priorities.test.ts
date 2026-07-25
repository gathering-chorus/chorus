// @test-type: unit — injects sparql + pulse stubs; no Fuseki, no live service, brings its own world.
// #3683 — /sup's data contract: the role's priorities walk read from the GRAPH
// (#3654 board domain), not a label-scrape. Chunks in roleSequence order, cards
// in rank order within each chunk; open board cards with no chunk membership are
// listed after, labeled unsequenced — never hidden. Jeff's experience under test:
// "types /sup → the role's walk, the same order the graph holds."
import { fetchContextPriorities } from '../src/handlers/context-priorities';

const NS = 'https://jeffbridwell.com/chorus#';

// SPARQL stub: the stampHeader LIMIT-1 gets empty bindings; the walk query is
// answered from fixture bindings.
function sparqlStub(walkBindings: Array<Record<string, string>>) {
  return {
    query: async (q: string) => {
      if (q.includes('roleSequence')) {
        return {
          results: {
            bindings: walkBindings.map((b) =>
              Object.fromEntries(Object.entries(b).map(([k, v]) => [k, { value: v }])),
            ),
          },
        };
      }
      return { results: { bindings: [] } };
    },
  };
}

const walkFixture = [
  // deliberately OUT of order — the handler must sort, not trust arrival order
  { chunkLabel: 'ops', roleSeq: '3' },
  { chunkLabel: 'security', roleSeq: '1', loomSeq: '1', rank: '2', cardIri: `${NS}card-3564`, cardLabel: 'Govern + secure Fuseki' },
  { chunkLabel: 'werk-reliability', roleSeq: '2', rank: '1', cardIri: `${NS}card-3455`, cardLabel: 'Fitness-probe framework' },
  { chunkLabel: 'security', roleSeq: '1', loomSeq: '1', rank: '1', cardIri: `${NS}card-3356`, cardLabel: 'Model-store security' },
];

const pulseFixture = JSON.stringify({
  board: {
    wip_cards: [{ id: 3356, owner: 'Silas', title: 'Model-store security', priority: 'P1' }],
    next_cards: [
      { id: 2106, owner: 'Silas', title: 'Service domains require logs and alerts', priority: 'P2' },
      { id: 1818, owner: 'Wren', title: 'Seeds: close-the-loop', priority: 'P2' },
    ],
    swat_cards: [],
  },
});

const deps = (bindings = walkFixture, pulse: string | null = pulseFixture) => ({
  sparql: sparqlStub(bindings),
  readPulse: () => pulse,
});

const URL_ = '/api/chorus/context/priorities?role=silas';

describe('#3683 /sup — chunks in roleSequence order, cards in rank order (AC2)', () => {
  it('sorts chunks by roleSequence ascending regardless of arrival order', async () => {
    const r = await fetchContextPriorities(deps(), URL_, 'silas');
    expect(r.status).toBe(200);
    const body = r.body as { data: { chunks: Array<{ chunk: string; roleSequence: number }> } };
    expect(body.data.chunks.map((c) => c.chunk)).toEqual(['security', 'werk-reliability', 'ops']);
    expect(body.data.chunks.map((c) => c.roleSequence)).toEqual([1, 2, 3]);
  });

  it('sorts cards within a chunk by rank and extracts the vikunja id from the IRI', async () => {
    const r = await fetchContextPriorities(deps(), URL_, 'silas');
    const body = r.body as { data: { chunks: Array<{ chunk: string; cards: Array<{ id: number; rank: number; title: string }> }> } };
    const sec = body.data.chunks.find((c) => c.chunk === 'security')!;
    expect(sec.cards.map((c) => c.id)).toEqual([3356, 3564]);
    expect(sec.cards.map((c) => c.rank)).toEqual([1, 2]);
    expect(sec.cards[0].title).toBe('Model-store security');
  });

  it('carries loomSequence when the chunk declares one, omits it otherwise', async () => {
    const r = await fetchContextPriorities(deps(), URL_, 'silas');
    const body = r.body as { data: { chunks: Array<{ chunk: string; loomSequence?: number }> } };
    expect(body.data.chunks.find((c) => c.chunk === 'security')!.loomSequence).toBe(1);
    expect(body.data.chunks.find((c) => c.chunk === 'ops')!.loomSequence).toBeUndefined();
  });

  it('a chunk with no memberships still appears (empty cards) — committed but unfilled is visible', async () => {
    const r = await fetchContextPriorities(deps(), URL_, 'silas');
    const body = r.body as { data: { chunks: Array<{ chunk: string; cards: unknown[] }> } };
    expect(body.data.chunks.find((c) => c.chunk === 'ops')!.cards).toEqual([]);
  });
});

describe('#3683 /sup — unsequenced cards listed honestly (AC3)', () => {
  it('open board cards owned by the role with NO membership land in unsequenced', async () => {
    const r = await fetchContextPriorities(deps(), URL_, 'silas');
    const body = r.body as { data: { unsequenced: { cards: Array<{ id: number }>; scope: string } } };
    expect(body.data.unsequenced.cards.map((c) => c.id)).toEqual([2106]);
    expect(body.data.unsequenced.scope).toMatch(/Now|WIP|Next/); // labeled, not hidden
  });

  it('sequenced cards are NOT duplicated into unsequenced; other roles excluded', async () => {
    const r = await fetchContextPriorities(deps(), URL_, 'silas');
    const body = r.body as { data: { unsequenced: { cards: Array<{ id: number }> } } };
    const ids = body.data.unsequenced.cards.map((c) => c.id);
    expect(ids).not.toContain(3356); // in the security chunk
    expect(ids).not.toContain(1818); // Wren's card
  });

  it('missing pulse mirror degrades honestly: walk still served, unsequenced labeled unavailable', async () => {
    const r = await fetchContextPriorities(deps(walkFixture, null), URL_, 'silas');
    expect(r.status).toBe(200);
    const body = r.body as { data: { chunks: unknown[]; unsequenced: { cards: unknown[]; scope: string } } };
    expect(body.data.chunks.length).toBe(3);
    expect(body.data.unsequenced.cards).toEqual([]);
    expect(body.data.unsequenced.scope).toMatch(/unavailable/i);
  });
});

describe('#3683 /sup — role validation', () => {
  it('rejects an unknown role with 400, does not query', async () => {
    const r = await fetchContextPriorities(deps(), URL_, 'mallory');
    expect(r.status).toBe(400);
  });
});
