// @test-type: unit — pure: the AUTH relay address from its inputs; no relay.
// @domain: messages
// @card: #4657
// @owner: silas
/**
 * #4657 — the relay accepts one address in a NIP-42 AUTH (its RELAY_URL). Once
 * that is the public wss:// one, v2 still dials the local tunnel, so the room,
 * Jeff's publisher and the signer transport all name BUZZ_AUTH_RELAY_URL.
 * (src/relay-client authRelayFor, used by src/jeff-publisher and src/signer-transport)
 */
import { authRelayFor } from '../src/relay-client';

const PUBLIC = 'wss://buzz.lightlifeurbangardens.com';

describe('#4657 authRelayFor', () => {
  test('a stated relay address is the AUTH address, whatever v2 dials', () => {
    expect(authRelayFor('ws://127.0.0.1:13000', '192.168.86.242:3000', { BUZZ_AUTH_RELAY_URL: PUBLIC })).toBe(PUBLIC);
  });

  // NEGATIVE PROOF: unset keeps today's address exactly, so landing with the setting off changes nothing
  test('unset, the tunnel Host with the dial scheme, or the dial address', () => {
    expect(authRelayFor('ws://127.0.0.1:13000', '192.168.86.242:3000', {})).toBe('ws://192.168.86.242:3000');
    expect(authRelayFor('wss://127.0.0.1:13000', 'relay.example', {})).toBe('wss://relay.example');
    expect(authRelayFor('ws://192.168.86.242:3000', undefined, {})).toBe('ws://192.168.86.242:3000');
  });
});
