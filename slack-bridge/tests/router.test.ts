import { MessageRouter } from '../src/router';
import { SlackMessage } from '../src/channel-monitor';
import { RolesConfig } from '../src/config';

const testConfig: RolesConfig = {
  roles: [
    { name: 'silas', channel: 'silas', claudeMdPath: '/team/architect/CLAUDE.md', memoryPath: '/memory/silas/MEMORY.md', briefsPath: '/team/architect/briefs', maxCallsPerHour: 15 },
    { name: 'wren', channel: 'wren', claudeMdPath: '/team/product-manager/CLAUDE.md', memoryPath: '/memory/wren/MEMORY.md', briefsPath: '/team/product-manager/briefs', maxCallsPerHour: 15 },
    { name: 'kade', channel: 'kade', claudeMdPath: '/team/engineer/CLAUDE.md', memoryPath: '/memory/kade/MEMORY.md', briefsPath: '/team/engineer/briefs', maxCallsPerHour: 15 },
  ],
  sharedChannel: 'all-gathering',
  pollIntervalMs: 30000,
  globalMaxCallsPerHour: 30,
};

function makeMessage(channelName: string, text: string): SlackMessage {
  return { text, user: 'U123', ts: '1234567890.000001', channel: 'C123', channelName };
}

describe('MessageRouter', () => {
  const router = new MessageRouter(testConfig);

  describe('direct channel routing', () => {
    it('routes #silas messages to Silas', () => {
      const results = router.route(makeMessage('silas', 'hey, quick question'));
      expect(results).toHaveLength(1);
      expect(results[0].role.name).toBe('silas');
    });

    it('routes #wren messages to Wren', () => {
      const results = router.route(makeMessage('wren', 'status update?'));
      expect(results).toHaveLength(1);
      expect(results[0].role.name).toBe('wren');
    });

    it('routes #kade messages to Kade', () => {
      const results = router.route(makeMessage('kade', 'you there?'));
      expect(results).toHaveLength(1);
      expect(results[0].role.name).toBe('kade');
    });
  });

  describe('shared channel routing', () => {
    it('routes to Silas when name mentioned in #all-gathering', () => {
      const results = router.route(makeMessage('all-gathering', 'Silas — thoughts on this?'));
      expect(results).toHaveLength(1);
      expect(results[0].role.name).toBe('silas');
    });

    it('routes to multiple roles when multiple names mentioned', () => {
      const results = router.route(makeMessage('all-gathering', 'Silas and Kade — can you check?'));
      expect(results).toHaveLength(2);
      const names = results.map(r => r.role.name).sort();
      expect(names).toEqual(['kade', 'silas']);
    });

    it('returns empty when no role named in #all-gathering', () => {
      const results = router.route(makeMessage('all-gathering', 'hey team, general update'));
      expect(results).toHaveLength(0);
    });

    it('handles case-insensitive matching', () => {
      const results = router.route(makeMessage('all-gathering', 'WREN what do you think'));
      expect(results).toHaveLength(1);
      expect(results[0].role.name).toBe('wren');
    });
  });

  describe('unknown channels', () => {
    it('returns empty for unknown channels', () => {
      const results = router.route(makeMessage('random', 'hello'));
      expect(results).toHaveLength(0);
    });
  });
});
