// @test-type: unit — a temp turn file; no spine write (NODE_ENV=test), no store
// @domain: cards
// @card: #4637
// @owner: silas
// #4637 AC4 — src/events.ts: a card action taken in a turn Jeff's line woke
// names that line (started_by_line), so the #4638 tree can hang the card off his prompt.
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { readTurnLine, spineArgs, TURN_FRESH_MS } from '../src/events';

const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'cards-turn-4637-'));
const write = (role: string, body: object) => fs.writeFileSync(path.join(dir, `turn-${role}.json`), JSON.stringify(body));
const now = Date.now();

test('a card action in a turn a Jeff line woke names the line', () => {
  write('silas', { turn_id: 't1', woken_by_line: '57629', at: Math.floor(now / 1000) });
  expect(readTurnLine('silas', dir, now)).toBe('57629');
  expect(spineArgs('card.item.moved', 'silas', { card_id: '4637' }, readTurnLine('silas', dir, now)))
    .toEqual(['card.item.moved', 'silas', 'card_id=4637', 'started_by_line=57629', 'appName=cards', 'component=cli']);
});

test('negative proof: a nudge-started turn, a stale turn or no turn file gives no line', () => {
  write('kade', { turn_id: 't2', woken_by_line: null, at: Math.floor(now / 1000) });
  expect(readTurnLine('kade', dir, now)).toBeUndefined();
  write('wren', { turn_id: 't3', woken_by_line: '1', at: Math.floor((now - TURN_FRESH_MS - 1000) / 1000) });
  expect(readTurnLine('wren', dir, now)).toBeUndefined();
  expect(readTurnLine('nobody', dir, now)).toBeUndefined();
  expect(spineArgs('card.item.moved', 'kade', { card_id: '1' }, undefined)).not.toContain('started_by_line');
});
