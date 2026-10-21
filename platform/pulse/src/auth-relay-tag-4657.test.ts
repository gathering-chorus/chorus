// @test-type: unit — pure: the AUTH relay tag from its inputs; no relay.
// @domain: messages
// @card: #4657
// @owner: silas
/**
 * #4657 — the relay accepts one address in a NIP-42 AUTH (its RELAY_URL). When
 * that becomes the public wss:// one, pulse still dials the local tunnel, so the
 * tag must come from BUZZ_AUTH_RELAY_URL, not the dial address.
 * Used by src/buzz-writer, src/buzz-reader and src/buzz-day-counts.
 */
import { authRelayTag } from './buzz-writer';

const PUBLIC = 'wss://buzz.lightlifeurbangardens.com';

describe('#4657 authRelayTag', () => {
  test('a stated relay address is the tag, whatever pulse dials', () => {
    expect(authRelayTag('ws://127.0.0.1:13000', '192.168.86.242:3000', { BUZZ_AUTH_RELAY_URL: PUBLIC })).toBe(PUBLIC);
  });

  // NEGATIVE PROOF: unset keeps today's tag exactly (ws://<Host>), so the flag-off land changes nothing
  test('unset, the tag is the tunnel Host as ws://, as before', () => {
    expect(authRelayTag('ws://127.0.0.1:13000', '192.168.86.242:3000', {})).toBe('ws://192.168.86.242:3000');
    expect(authRelayTag('ws://192.168.86.242:3000', undefined, {})).toBe('ws://192.168.86.242:3000');
  });

  test('a blank value counts as unset', () => {
    expect(authRelayTag('ws://127.0.0.1:13000', '192.168.86.242:3000', { BUZZ_AUTH_RELAY_URL: '  ' })).toBe('ws://192.168.86.242:3000');
  });
});
