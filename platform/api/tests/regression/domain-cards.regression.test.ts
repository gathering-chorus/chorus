// @test-type: unit — the board's cards injected as the board cache holds them; no live service.
// @domain: cards
// @card: #5021
/**
 * Regression (DEC-2208): the Cards fold's answer, /api/chorus/domain/:name/cards, from
 * handlers/domain-cards.ts, held to a golden. The fold read /api/chorus/domain/:name, which knew
 * 25 v1 names (principles: 404) and answered its cards as an object the fold could not list.
 */
import * as fs from 'fs';
import * as path from 'path';
import { fetchDomainCards, type BoardCard } from '../../src/handlers/domain-cards';

const GOLDEN_PATH = path.join(__dirname, 'golden', 'domain-cards-principles.json');

const board: BoardCard[] = [
  { id: '5017', title: 'Switch principles to the athena-gen API', status: 'Done', owner: 'Kade', priority: 'P2', tags: 'Kade|P2|domain:principles|type:enhance' },
  { id: '5020', title: 'Domain lint', status: 'Done', owner: 'Kade', priority: 'P2', tags: 'Kade|P2|domain:chorus|type:enhance' },
  { id: '5021', title: 'Make the principles domain conformant', status: 'WIP', owner: 'Kade', priority: 'P2', tags: 'Kade|P2|domain:principles|type:fix|sequence:athena' },
];

test('principles → its cards, open first, matching the golden', () => {
  const r = fetchDomainCards(() => board, 'principles');
  expect(r.status).toBe(200);
  expect(r.body).toEqual(JSON.parse(fs.readFileSync(GOLDEN_PATH, 'utf-8')));
});
