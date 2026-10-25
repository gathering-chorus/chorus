// @test-type: unit — pure attribution rules; no store, no door
// @domain: messages
// @card: #4637
// @owner: silas
// #4637 AC1/AC2 — src/sender.ts: only real actors send; an alert is attributed to the role that owns it.
import { attributeSender, markAlert } from './sender';

const actors = ['abby-normal', 'borg-queen', 'kade', 'silas', 'wren'];

test('an actor sends as itself', () => {
  expect(attributeSender('kade', 'silas', undefined, actors)).toEqual({ kind: 'actor', from: 'kade' });
  expect(attributeSender('jeff', 'wren', undefined, actors)).toEqual({ kind: 'actor', from: 'jeff' });
  expect(attributeSender('borg-queen', 'wren', undefined, actors)).toEqual({ kind: 'actor', from: 'borg-queen' });
});

test('a machine alert is the owning role\'s, never a nudge from system', () => {
  expect(attributeSender('system', 'kade', undefined, actors)).toEqual({ kind: 'alert', from: 'kade', source: 'system' });
  expect(attributeSender('system', 'jeff', 'silas', actors)).toEqual({ kind: 'alert', from: 'silas', source: 'system' });
  expect(markAlert('kade', 'system', '[nudge from system | 2026-10-24 20:15 Boston] crawler-out-of-sync: …', '2026-10-24 21:00'))
    .toBe('[alert for kade · from system | 2026-10-24 21:00 Boston] crawler-out-of-sync: …');
});

// NEGATIVE PROOF (AC5): a message with no real author is refused by name and stored nowhere.
test('negative proof: no author, a bridge, or an alert with no owner is refused by name', () => {
  for (const from of ['', undefined, 'bridge', 'unattributed', 'chorus-mcp']) {
    const r = attributeSender(from, 'silas', undefined, actors);
    expect(r.kind).toBe('refused');
    if (r.kind === 'refused') {
      expect(r.status).toBe(422);
      expect(r.body.message).toContain('is not an actor');
    }
  }
  const r = attributeSender('system', 'nobody', 'also-nobody', actors);
  expect(r.kind).toBe('refused');
  if (r.kind === 'refused') expect(r.body.message).toContain('names no owning role');
});
