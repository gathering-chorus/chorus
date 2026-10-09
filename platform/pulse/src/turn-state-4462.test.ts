// @domain: messages
// @test-type: unit — temp directory with turn files; no live services
// @card: #4462
// @owner: wren
/**
 * #4462 — pulse holds a nudge while the target is mid-turn, read from
 * <role>.turn.json. A missing or broken marker must read idle: a bad file can
 * never stop delivery.
 */
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { readTurnState } from './session-registry';

let dir: string;
beforeEach(() => { dir = fs.mkdtempSync(path.join(os.tmpdir(), 'turn-4462-')); });
afterEach(() => { fs.rmSync(dir, { recursive: true, force: true }); });

test('a busy marker reads busy, with its start time', () => {
  fs.writeFileSync(path.join(dir, 'kade.turn.json'), JSON.stringify({ busy: true, since: '2026-10-09T12:00:00Z' }));
  expect(readTurnState('kade', dir)).toEqual({ busy: true, since: '2026-10-09T12:00:00Z' });
});

test('negative proof: no file, broken JSON, or a busy field that is not a boolean all read idle', () => {
  expect(readTurnState('kade', dir)).toEqual({ busy: false });
  fs.writeFileSync(path.join(dir, 'kade.turn.json'), '{not json');
  expect(readTurnState('kade', dir)).toEqual({ busy: false });
  fs.writeFileSync(path.join(dir, 'kade.turn.json'), JSON.stringify({ busy: 'yes' }));
  expect(readTurnState('kade', dir)).toEqual({ busy: false });
  fs.writeFileSync(path.join(dir, 'kade.turn.json'), 'null');
  expect(readTurnState('kade', dir)).toEqual({ busy: false });
});
