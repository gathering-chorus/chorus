// #2876 + #3267: cards now emits spine events via the `chorus-log` subprocess
// (not the removed chorus-sdk). The card_id-coercion behavior is tested against
// the pure `spineArgs` builder (the chorus-log argv) instead of a mocked emit.
// Note: the FINAL JSON typing of card_id (`"card_id":NNN` unquoted) is now
// chorus-log's responsibility; this test pins the args contract cards hands it.

import { emitSpineEvent, spineArgs } from '../src/events';

describe('emitSpineEvent / spineArgs card_id contract (#2876, #3267)', () => {
  it('emitSpineEvent is a no-op under jest (#2241 spine-leak guard preserved)', () => {
    // NODE_ENV=test under jest → returns without spawning chorus-log.
    expect(emitSpineEvent('card.demo.started', 'silas', { card_id: '2876' })).toBeUndefined();
  });

  it('builds the chorus-log argv: event, role, kv, appName/component', () => {
    const args = spineArgs('card.demo.started', 'silas', { card_id: '2876', title: 'x' });
    expect(args[0]).toBe('card.demo.started');
    expect(args[1]).toBe('silas');
    expect(args).toContain('card_id=2876');
    expect(args).toContain('title=x');
    expect(args).toContain('appName=cards');
    expect(args).toContain('component=cli');
  });

  it('coerces numeric-string card_id to a numeric token (#2876)', () => {
    const args = spineArgs('e', 'r', { card_id: '2876' });
    expect(args).toContain('card_id=2876');
    // not the quoted/string artifact a non-coerced value would risk downstream
    expect(args.some((a) => a === 'card_id="2876"')).toBe(false);
  });

  it('passes integer card_id through unchanged', () => {
    expect(spineArgs('e', 'r', { card_id: 2876 })).toContain('card_id=2876');
  });

  it('leaves non-numeric card_id strings alone (defensive against junk input)', () => {
    expect(spineArgs('e', 'r', { card_id: 'not-a-number' })).toContain('card_id=not-a-number');
  });
});
