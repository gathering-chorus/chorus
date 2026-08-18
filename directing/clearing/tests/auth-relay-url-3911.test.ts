// @test-type: unit — pure URL derivation; no socket, no relay.
/**
 * #3911 — the AUTH event must name the relay, not the tunnel.
 *
 * NIP-42 has the relay check the `relay` tag against its own identity. We reach
 * the relay through a loopback tunnel, so the dial address is 127.0.0.1 and the
 * AUTH event was claiming that. Every attempt came back `accepted:false` with
 * "auth-required: authenticate before subscribing", 249 times over, and the
 * Clearing simply looked quiet.
 *
 * NEGATIVE PROOF (#3734): `claims the dial address when no host is configured`
 * is the shipped state — the one that fails at the relay. Asserting it means the
 * fallback cannot silently become the default again, which is exactly how the
 * Host-header fix ended up landing dead for a day.
 */
import { authRelayUrl } from '../src/buzz-room-wiring';

describe('#3911 AUTH relay tag', () => {
  it('claims the relay own address, not the tunnel we dialed', () => {
    expect(authRelayUrl('ws://127.0.0.1:13000', '192.168.86.242:3000'))
      .toBe('ws://192.168.86.242:3000');
  });

  it('NEGATIVE: with no host configured it claims the dial address — the refused state', () => {
    expect(authRelayUrl('ws://127.0.0.1:13000', undefined))
      .toBe('ws://127.0.0.1:13000');
  });

  it('preserves a secure scheme rather than downgrading it', () => {
    expect(authRelayUrl('wss://127.0.0.1:13000', 'relay.example:443'))
      .toBe('wss://relay.example:443');
  });

  it('a direct dial with a matching host is unchanged in effect', () => {
    expect(authRelayUrl('ws://192.168.86.242:3000', '192.168.86.242:3000'))
      .toBe('ws://192.168.86.242:3000');
  });
});
