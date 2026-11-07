// @test-type: unit — pure: the AUTH relay address from its inputs; no relay.
// @domain: messages
// @card: #4657
// @owner: silas
/**
 * #4657 — v1's AUTH names BUZZ_AUTH_RELAY_URL when it is set (the relay accepts
 * only its RELAY_URL), else the tunnel Host as before. (src/buzz-room-wiring authRelayUrl; src/buzz-relay authFrame reads the same setting)
 */
import { authRelayUrl } from '../src/buzz-room-wiring';

const PUBLIC = 'wss://buzz.lightlifeurbangardens.com';

describe('#4657 v1 authRelayUrl', () => {
  test('a stated relay address wins over the dial address and the tunnel Host', () => {
    expect(authRelayUrl('ws://127.0.0.1:13000', '192.168.86.242:3000', { BUZZ_AUTH_RELAY_URL: PUBLIC })).toBe(PUBLIC);
  });

  // NEGATIVE PROOF: unset is today's behaviour
  test('unset, the tunnel Host as ws://', () => {
    expect(authRelayUrl('ws://127.0.0.1:13000', '192.168.86.242:3000', {})).toBe('ws://192.168.86.242:3000');
  });
});
