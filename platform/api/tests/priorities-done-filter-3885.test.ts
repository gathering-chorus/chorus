// @test-type: unit — pure handler with injected sparql/pulse/status stubs; no DB, no HTTP.
/**
 * #3885 — the walk shows what is left, uncluttered by what is done.
 *
 * Jeff, 2026-08-17: "whats left, uncluttered by what is done." Silas's walk was
 * listing five landed rows as open work and misordering his pulls.
 *
 * NEGATIVE PROOFS (#3734), two of them, because the obvious implementation is
 * wrong in a way that looks right:
 *
 *  - `a parked card stays in the walk` — the tempting filter is "not in the
 *    board mirror's open lanes", but that mirror carries only wip/next/swat, so
 *    absence means Done OR Later. Of the five rows reported stale, TWO were
 *    Later — real work that filter would have hidden. Queried live before
 *    writing this: 3615/3687/3690 Done, 3616/3691 Later.
 *  - `a failed status read keeps every card` — if the board is unreachable, an
 *    empty map must not read as "everything is done" and empty the walk.
 */
import { fetchContextPriorities } from '../src/handlers/context-priorities';

const NS = 'https://jeffbridwell.com/chorus#';

/** Three chunk rows: a Done card, a parked card, an open card. */
function walkBindings() {
  const row = (label: string, seq: string, rank: string, id: string, title: string) => ({
    chunkLabel: { value: label },
    roleSeq: { value: seq },
    rank: { value: rank },
    cardIri: { value: `${NS}card-${id}` },
    cardLabel: { value: title },
  });
  return [
    row('security', '1', '1', '3690', 'landed thing'),
    row('security', '1', '2', '3691', 'parked thing'),
    row('security', '1', '3', '3885', 'open thing'),
  ];
}

type Reader = (ids: number[]) => Promise<Map<number, string>>;

function deps(readCardStatuses?: Reader) {
  return {
    sparql: {
      query: async (q: string) => (q.includes('chorus:Chunk')
        ? { results: { bindings: walkBindings() } }
        : { results: { bindings: [] } }),
    },
    readPulse: () => JSON.stringify({ board: { wip_cards: [], next_cards: [], swat_cards: [] } }),
    owl: async () => ({ data: [] }),
    ...(readCardStatuses && { readCardStatuses }),
  } as unknown as Parameters<typeof fetchContextPriorities>[0];
}

async function cardsFor(readCardStatuses?: Reader) {
  const r = await fetchContextPriorities(
    deps(readCardStatuses),
    '/api/chorus/context/priorities?role=silas',
    'silas',
  );
  // The handler wraps its payload in the standard envelope: { data: { chunks } }.
  const body = r.body as { data?: { chunks?: Array<{ chunk: string; cards: Array<{ id: number }> }> } };
  const chunk = (body.data?.chunks ?? []).find((c) => c.chunk === 'security');
  return (chunk?.cards ?? []).map((c) => c.id);
}

describe('#3885 Done leaves the walk', () => {
  it('drops a Done card', async () => {
    const ids = await cardsFor(async () => new Map([[3690, 'Done'], [3691, 'Later'], [3885, 'Next']]));
    expect(ids).not.toContain(3690);
  });

  it('NEGATIVE: a parked (Later) card STAYS — it is still work', async () => {
    const ids = await cardsFor(async () => new Map([[3690, 'Done'], [3691, 'Later'], [3885, 'Next']]));
    expect(ids).toContain(3691);
    expect(ids).toContain(3885);
  });

  it("drops Won't Do as well as Done", async () => {
    const ids = await cardsFor(async () => new Map([[3690, "Won't Do"], [3691, 'Later'], [3885, 'Next']]));
    expect(ids).not.toContain(3690);
    expect(ids).toContain(3691);
  });

  it('NEGATIVE: a failed status read keeps every card, never empties the walk', async () => {
    const ids = await cardsFor(async () => new Map());
    expect(ids).toEqual([3690, 3691, 3885]);
  });

  it('with no status reader wired at all, the walk is unchanged', async () => {
    expect(await cardsFor(undefined)).toEqual([3690, 3691, 3885]);
  });

  it('rank order survives the filter', async () => {
    const ids = await cardsFor(async () => new Map([[3690, 'Done']]));
    expect(ids).toEqual([3691, 3885]);
  });
});
