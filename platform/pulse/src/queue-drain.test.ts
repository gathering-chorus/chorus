// @test-type: integration — #3700: the queue→drain loop against a real tmp
// sqlite store (brings its own world, no live messages.db).
import { MessageStore } from './store';
import { mkdtempSync, rmSync } from 'fs';
import os from 'os';
import path from 'path';

describe('#3700 queued state + drain', () => {
  let dir: string; let store: MessageStore;
  beforeEach(() => {
    dir = mkdtempSync(path.join(os.tmpdir(), 'pulse-3700-'));
    store = new MessageStore(path.join(dir, 'messages.db'));
  });
  afterEach(() => { rmSync(dir, { recursive: true, force: true }); });

  test('markQueued parks a row out of the pending scan; drain releases it in order', () => {
    const a = store.sendNudge('silas', 'wren', 'first');
    const b = store.sendNudge('silas', 'wren', 'second');
    store.markQueued(a, 'target-busy');
    store.markQueued(b, 'target-busy');
    expect(store.getPendingDeliveries().map(r => r.id)).toEqual([]); // parked
    const released = store.drainQueued('wren');
    expect(released).toBe(2);
    expect(store.getPendingDeliveries().map(r => r.content)).toEqual(['first', 'second']); // order kept
  });

  test('drain only touches the named role', () => {
    const a = store.sendNudge('silas', 'wren', 'w');
    const k = store.sendNudge('silas', 'kade', 'k');
    store.markQueued(a, 'target-busy'); store.markQueued(k, 'target-busy');
    expect(store.drainQueued('wren')).toBe(1);
    expect(store.getPendingDeliveries().map(r => r.to)).toEqual(['wren']);
  });

  test('migration is idempotent (second open of the same db is a no-op)', () => {
    const again = new MessageStore(path.join(dir, 'messages.db'));
    expect(again.drainQueued('wren')).toBe(0); // schema intact, no throw
  });
});
