// @test-type: unit — pure: who receives an unaddressed Jeff message; no door, no network.
// @card: #4875
// @owner: abby-normal
// @domain: messages
// AC5 (Jeff 11-03 07:15): an unaddressed Jeff message reaches ONE moderator, who answers for the
// team; the moderator is a configurable property (global `room.moderator`), never hard-coded.
// Enforced at delivery (Kade): the other roles never get the prompt.

import { MessageRouter } from '../src/router';
import { moderatorFromProperties } from '../src/moderator';
import { ClearingMessage } from '../src/types';

const jeffSays = (text: string): ClearingMessage => ({
  id: `m-${text.length}`, from: 'jeff', text, ts: new Date().toISOString(), type: 'jeff-input', visible: true,
});
const props = (value: string, key = 'room.moderator') => ({
  data: [{ name: 'prop-room-moderator', propertyKey: key, propertyScope: 'global', propertyValueType: 'string', propertyValue: value }],
});

describe('#4875 AC5: one moderator for unaddressed Jeff messages', () => {
  test('the moderator alone receives an unaddressed message', () => {
    const r = new MessageRouter(50, ['wren', 'kade', 'silas', 'abby-normal']);
    r.setModerator(moderatorFromProperties(props('wren')));
    expect(r.extractNudges(jeffSays('how do we prove it works?')).map((n) => n.to)).toEqual(['wren']);
  });

  test('negative proof: flip the property and a different role receives it', () => {
    const r = new MessageRouter(50, ['wren', 'kade', 'silas', 'abby-normal']);
    r.setModerator(moderatorFromProperties(props('kade')));
    expect(r.extractNudges(jeffSays('how do we prove it works?')).map((n) => n.to)).toEqual(['kade']);
  });

  test('an @-addressed message still goes to exactly who Jeff named', () => {
    const r = new MessageRouter(50, ['wren', 'kade', 'silas']);
    r.setModerator('wren');
    expect(r.extractNudges(jeffSays('@silas what is that alert')).map((n) => n.to)).toEqual(['silas']);
  });

  test('negative proof: a moderator that is not a known role is ignored, never a dead letter', () => {
    const r = new MessageRouter(50, ['wren', 'kade']);
    r.setModerator(moderatorFromProperties(props('nobody')));
    expect(r.extractNudges(jeffSays('hello')).map((n) => n.to).sort()).toEqual(['kade', 'wren']);
  });

  test('the property reader takes only room.moderator rows', () => {
    expect(moderatorFromProperties(props('wren'))).toBe('wren');
    expect(moderatorFromProperties(props('wren', 'response.word.cap'))).toBeUndefined();
    expect(moderatorFromProperties(null)).toBeUndefined();
    expect(moderatorFromProperties({ data: 'x' })).toBeUndefined();
  });
});
