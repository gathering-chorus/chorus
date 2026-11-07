// @test-type: unit
// @card: #4749
// @domain: messages

import { toClearingMessage, roomOf, kindOf } from '../src/converters';
import { NostrEvent } from '../src/types';

describe('converters (#4749 cycle break)', () => {
  it('extracts room tag or defaults to team', () => {
    const ev1: NostrEvent = {
      id: '1',
      pubkey: 'pk1',
      created_at: 1000,
      kind: 9,
      tags: [['room', 'silas']],
      content: 'hi',
      sig: 'sig',
    };
    expect(roomOf(ev1)).toBe('silas');

    const ev2: NostrEvent = {
      id: '2',
      pubkey: 'pk1',
      created_at: 1000,
      kind: 9,
      tags: [],
      content: 'hi',
      sig: 'sig',
    };
    expect(roomOf(ev2)).toBe('team');
  });

  it('extracts kind tag for nudge or alert', () => {
    expect(kindOf({ tags: [['kind', 'nudge']] })).toBe('nudge');
    expect(kindOf({ tags: [['kind', 'alert']] })).toBe('alert');
    expect(kindOf({ tags: [['kind', 'other']] })).toBeUndefined();
    expect(kindOf({})).toBeUndefined();
  });

  it('converts NostrEvent to ClearingMessage', () => {
    const ev: NostrEvent = {
      id: 'note-1',
      pubkey: 'pk-kade',
      created_at: 1700000000,
      kind: 9,
      tags: [['kind', 'nudge'], ['signed', 'note-original'], ['room', 'team']],
      content: 'hello abby',
      sig: 'sig1',
    };
    const msg = toClearingMessage(ev, 'kade', true, 'kade');
    expect(msg.id).toBe('note-original');
    expect(msg.from).toBe('kade');
    expect(msg.text).toBe('hello abby');
    expect(msg.kind).toBe('nudge');
    expect(msg.room).toBe('team');
    expect(msg.tick).toBe(true);
    expect(msg.signer).toBe('kade');
    expect(msg.type).toBe('role-response');
  });
});
