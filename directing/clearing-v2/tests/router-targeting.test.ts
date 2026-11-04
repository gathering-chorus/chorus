// @test-type: unit — Clearing v2 router direct room targeting (#4877 AC3, AC4)
// @card: #4877
// @owner: abby-normal
// @domain: messages
import { MessageRouter } from '../src/router';
import { ClearingMessage } from '../src/types';

describe('router targeting (#4877 AC3, AC4)', () => {
  it('AC3: direct room message without explicit mention routes exclusively to the direct room role', () => {
    const router = new MessageRouter(50, ['jeff', 'wren', 'silas', 'kade', 'abby-normal']);
    const msg: ClearingMessage = {
      id: 'm1',
      from: 'jeff',
      text: 'how is your card going?',
      ts: new Date().toISOString(),
      type: 'jeff-input',
      visible: true,
      room: 'wren',
    };

    const nudges = router.extractNudges(msg);
    expect(nudges).toHaveLength(1);
    expect(nudges[0].to).toBe('wren');
    expect(nudges[0].from).toBe('jeff');
    expect(nudges[0].content).toBe('how is your card going?');
  });

  it('AC4: negative proof - unaddressed message in #team routes to moderator only (#4875), while direct room targets room role', () => {
    const router = new MessageRouter(50, ['jeff', 'wren', 'silas', 'kade', 'abby-normal']);
    router.setModerator('wren');

    // 1. Direct room to silas targets silas, NOT the moderator (wren)
    const directMsg: ClearingMessage = {
      id: 'm-direct',
      from: 'jeff',
      text: 'please inspect the logs',
      ts: new Date().toISOString(),
      type: 'jeff-input',
      visible: true,
      room: 'silas',
    };
    const directNudges = router.extractNudges(directMsg);
    expect(directNudges).toHaveLength(1);
    expect(directNudges[0].to).toBe('silas');

    // 2. Unaddressed message in #team routes to moderator only (wren)
    const teamMsg: ClearingMessage = {
      id: 'm-team',
      from: 'jeff',
      text: 'good morning everyone',
      ts: new Date().toISOString(),
      type: 'jeff-input',
      visible: true,
      room: 'team',
    };
    const teamNudges = router.extractNudges(teamMsg);
    expect(teamNudges).toHaveLength(1);
    expect(teamNudges[0].to).toBe('wren');

    // 3. Fallback broadcast: unaddressed message in #team when no moderator is set targets all non-jeff roles
    router.setModerator(undefined);
    const broadcastNudges = router.extractNudges(teamMsg);
    expect(broadcastNudges).toHaveLength(4);
    expect(broadcastNudges.map((n) => n.to)).toEqual(['wren', 'silas', 'kade', 'abby-normal']);
  });
});
