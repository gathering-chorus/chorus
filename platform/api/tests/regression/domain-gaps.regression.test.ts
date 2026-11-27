// @test-type: unit — the store's card numbers and the board's cards injected; no live service.
// @domain: domains
// @card: #5045
/**
 * Regression (DEC-2208): the Gaps fold's answer, /api/chorus/domain/:name/gaps, from
 * handlers/domain-gaps.ts, held to a golden. A gap on a domain is the card that will close it:
 * the Domain row holds the number (chorus:gapCard) and this read joins it to the board. The
 * golden holds all three standings a gap can have: open, closed (its card is done), and a number
 * the board does not hold.
 */
import * as fs from 'fs';
import * as path from 'path';
import { fetchDomainGaps, type GapsDeps } from '../../src/handlers/domain-gaps';
import type { BoardCard } from '../../src/handlers/domain-cards';

const GOLDEN_PATH = path.join(__dirname, 'golden', 'domain-gaps-principles.json');

const board: BoardCard[] = [
  { id: '5044', title: 'Make the Conformance fold answer at once', status: 'Done', owner: 'wren', priority: 'P2', tags: 'wren|P2|domain:chorus|type:fix' },
  { id: '5047', title: 'Fix the five value rows that name removed principles', status: 'Later', owner: 'wren', priority: 'P3', tags: 'wren|P3|domain:chorus|type:fix' },
  { id: '5021', title: 'a card the row does not name', status: 'WIP', owner: 'kade', priority: 'P2', tags: 'kade|P2|domain:principles' },
];
const store: GapsDeps['sparql'] = async () => ({ results: { bindings: ['99999', '5044', '5047'].map((n) => ({ n: { value: n } })) } });

test('principles → its gap cards as the board holds them, open first, matching the golden', async () => {
  const r = await fetchDomainGaps({ sparql: store, getCards: () => board }, 'principles');
  expect(r.status).toBe(200);
  expect(r.body).toEqual(JSON.parse(fs.readFileSync(GOLDEN_PATH, 'utf-8')));
});
