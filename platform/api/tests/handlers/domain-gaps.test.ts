// @test-type: unit — stubs the store and the board; no Fuseki, no live service.
// @domain: domains — placed by chorus-crawl's domain rules (#4538)
// #5045 AC2 — a gap on a domain is the card that will close it. Jeff, on his phone,
// 2026-11-25 15:47: "how do we resolve gaps". The fold showed a typed sentence nobody kept
// true. It now reads the cards the Domain row names (chorus:gapCard) and says, from the board
// itself, what each is and where it stands. Nothing about a gap is typed on the domain.
import { fetchDomainGaps, type GapsDeps } from '../../src/handlers/domain-gaps';

const card = (id: string, title: string, status: string, owner = 'wren') => ({ id, title, status, owner, priority: 'P3', tags: '' });
const store = (numbers: string[]) => (async () => ({ results: { bindings: numbers.map((n) => ({ n: { value: n } })) } })) as GapsDeps['sparql'];
type Body = { domain: string; gaps: Array<{ id: string; title: string | null; owner: string | null; status: string | null; state: string }>; error?: string };

describe('#5045 AC2 — a domain gap is the card that will close it', () => {
  test('each card the row names is answered with the board\'s own title, owner and status', async () => {
    const r = await fetchDomainGaps({
      sparql: store(['5047']),
      getCards: () => [card('5047', 'Fix five value rows naming removed principles', 'Later'), card('1', 'another card', 'WIP')],
    }, 'principles');
    expect(r.status).toBe(200);
    expect((r.body as Body).gaps).toEqual([
      { id: '5047', title: 'Fix five value rows naming removed principles', owner: 'wren', status: 'Later', state: 'open' },
    ]);
  });

  test('a gap whose card is Done is said to be closed, never shown as still open', async () => {
    const r = await fetchDomainGaps({
      sparql: store(['4085', '3148']),
      getCards: () => [card('4085', 'alerts read from the model', 'Done', 'silas'), card('3148', 'code crawl', "Won't Do", 'kade')],
    }, 'alerts');
    expect((r.body as Body).gaps.map((g) => [g.id, g.state])).toEqual([['3148', 'closed'], ['4085', 'closed']]);
  });

  test('NEGATIVE: a number the board does not hold is named as missing, with no title made up', async () => {
    const r = await fetchDomainGaps({ sparql: store(['99999']), getCards: () => [card('5047', 'x', 'Later')] }, 'principles');
    expect((r.body as Body).gaps).toEqual([{ id: '99999', title: null, owner: null, status: null, state: 'not on the board' }]);
  });

  test('NEGATIVE: a domain whose row names no card answers an empty list, not the board', async () => {
    const r = await fetchDomainGaps({ sparql: store([]), getCards: () => [card('5047', 'x', 'Later')] }, 'board');
    expect(r.status).toBe(200);
    expect((r.body as Body).gaps).toEqual([]);
  });

  test('NEGATIVE: a store that cannot be read is an error, never "no gaps"', async () => {
    const r = await fetchDomainGaps({
      sparql: (async () => { throw new Error('store down'); }) as GapsDeps['sparql'],
      getCards: () => [],
    }, 'principles');
    expect(r.status).toBe(502);
    expect((r.body as Body).error).toContain('store down');
    expect((r.body as Body).gaps).toBeUndefined();
  });

  test('NEGATIVE: a name that is not a domain name is refused before any read', async () => {
    let asked = 0;
    const r = await fetchDomainGaps({
      sparql: (async () => { asked++; return { results: { bindings: [] } }; }) as GapsDeps['sparql'],
      getCards: () => [],
    }, 'x> } INSERT { <a');
    expect(r.status).toBe(400);
    expect(asked).toBe(0);
  });
});
