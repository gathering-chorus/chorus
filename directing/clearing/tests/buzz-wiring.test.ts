// @test-type: unit — injected stubs (signer/socket/env); no live relay, no real key, brings its own world.
// #3696 — buzz-wiring assembly: the flag + config gating that decides whether the
// Clearing mirrors at all. Safe paths pinned here; the enabled mirror's network
// call is covered by the bridge/relay unit tests + the live proof. A valid test
// key (never real) exercises the signer build.
import { buildBuzzWiring } from '../src/buzz-wiring';

const KEY = '11'.repeat(32); // 64-hex, valid shape, NOT a real key
const noopLog = () => { /* silent in tests */ };

const env = (o: Record<string, string>) => o as unknown as NodeJS.ProcessEnv;

describe('#3696 buildBuzzWiring — flag + config gate', () => {
  it('flag OFF (no BUZZ_BRIDGE_ENABLED): disabled, mirror is a safe noop', () => {
    const w = buildBuzzWiring(env({}), noopLog);
    expect(w.enabled).toBe(false);
    expect(() => w.mirror({ from: 'wren', text: 'hi', ts: '2026-07-25T19:00:00Z', type: 'role-response', visible: true })).not.toThrow();
  });

  it('flag ON but missing relay/channel/key: refuses (disabled), never runs half-configured', () => {
    const w = buildBuzzWiring(env({ BUZZ_BRIDGE_ENABLED: '1', BUZZ_RELAY_URL: 'ws://r:3000' }), noopLog);
    expect(w.enabled).toBe(false); // channel + key absent
  });

  it('flag ON + fully configured: enabled, builds a signer from the key', () => {
    const w = buildBuzzWiring(
      env({ BUZZ_BRIDGE_ENABLED: '1', BUZZ_RELAY_URL: 'ws://r:3000', BUZZ_TEAM_CHANNEL: 'team-uuid', BUZZ_BRIDGE_NOSTR_KEY: KEY }),
      noopLog,
    );
    expect(w.enabled).toBe(true);
  });

  it('flag ON with a malformed key: the signer build refuses loudly (bad key never signs)', () => {
    expect(() =>
      buildBuzzWiring(
        env({ BUZZ_BRIDGE_ENABLED: '1', BUZZ_RELAY_URL: 'ws://r:3000', BUZZ_TEAM_CHANNEL: 'team-uuid', BUZZ_BRIDGE_NOSTR_KEY: 'not-hex' }),
        noopLog,
      ),
    ).toThrow(/hex/i);
  });
});