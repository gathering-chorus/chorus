// @test-type: unit
// @domain: cards
// @card: #5021
/**
 * #5021 — the Cards fold on /athena/domain.html reads /api/chorus/domain/:name/cards: the
 * board's domain:<name> cards as a list, for any domain. NEGATIVE: a card of another domain, or of
 * a name that only starts the same, is not listed; a bad name is refused.
 */
import { fetchDomainCards, type BoardCard } from '../../src/handlers/domain-cards';

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
