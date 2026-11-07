// @test-type: unit — verifies dynamic @-mention routing and borg-queen integration in clearing-v2 (#4724)
// @card: #4724
// @owner: abby-normal
// @domain: messages

import { MessageRouter } from '../src/router';
import { ClearingMessage } from '../src/types';

describe('#4724: Role mention routing and dynamic roles', () => {
  test('@borg-queen routes to borg-queen by default', () => {
    const router = new MessageRouter();
    const msg: ClearingMessage = {
      id: 'msg-bq-1',
      from: 'jeff',
      text: '@borg-queen where would u plug something like jev in…',
      ts: new Date().toISOString(),
      type: 'jeff-input',
      visible: true,
    };

    const nudges = router.extractNudges(msg);
    expect(nudges).toHaveLength(1);
    expect(nudges[0]).toEqual({
      from: 'jeff',
      to: 'borg-queen',
      content: msg.text,
    });
  });

  test('dynamic role registration: setKnownRoles enables newly configured role mentions', () => {
    const router = new MessageRouter(100, ['wren', 'silas']);
    const msg: ClearingMessage = {
      id: 'msg-custom-1',
      from: 'jeff',
      text: '@custom-bot run analysis',
      ts: new Date().toISOString(),
      type: 'jeff-input',
      visible: true,
    };

    // Before registration: unrecognized, falls back to unaddressed broadcast to known roles (wren, silas)
    const initialNudges = router.extractNudges(msg);
    expect(initialNudges.map((n) => n.to)).toEqual(['wren', 'silas']);

    // Register dynamically
    router.addKnownRole('custom-bot');
    const dynamicNudges = router.extractNudges(msg);
    expect(dynamicNudges).toHaveLength(1);
    expect(dynamicNudges[0].to).toBe('custom-bot');
  });

  test('NEGATIVE PROOF: mention of unknown role does not route to that role', () => {
    const router = new MessageRouter(100, ['jeff', 'wren', 'silas', 'borg-queen']);
    const msg: ClearingMessage = {
      id: 'msg-unknown-1',
      from: 'jeff',
      text: '@non-existent-agent please do something',
      ts: new Date().toISOString(),
      type: 'jeff-input',
      visible: true,
    };

    const nudges = router.extractNudges(msg);
    // Negative proof: none of the nudges are addressed to the non-existent role
    expect(nudges.some((n) => n.to === 'non-existent-agent')).toBe(false);
  });
});
