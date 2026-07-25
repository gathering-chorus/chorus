// @test-type: unit — injected stubs (signer/socket/env); no live relay, no real key, brings its own world.
// #3674 — the Clearing→Buzz mirror bridge. One-way tap: a visible Clearing
// message becomes a signed Nostr kind:9 event published to the relay channel.
// Spike-scoped, flag-off by default. Red-first (DEC-1674). Pure unit tests — the
// crypto is an INJECTED signer (real @noble impl lives in buzz-signer.ts, proven
// live where the relay rejects a bad sig); here a deterministic stub stands in.
import { buildKind9, BuzzBridge, type NostrSigner } from '../src/buzz-bridge';

const CHANNEL = 'clearing-uuid-1234';

// deterministic stub signer: id = a fixed transform of the serialization so the
// determinism assertion is meaningful without importing crypto.
const stubSigner = (): NostrSigner => ({
  pubkey: 'ab'.repeat(32),
  signEvent(serialized: string) {
    let h = 0;
    for (let i = 0; i < serialized.length; i++) h = (h * 31 + serialized.charCodeAt(i)) >>> 0;
    const id = h.toString(16).padStart(64, '0');
    return { id, sig: 'cd'.repeat(64) };
  },
});

describe('#3674 buildKind9 — Clearing message → signed Nostr event', () => {
  const msg = { from: 'wren', text: 'security is loom #1', ts: '2026-07-25T19:00:00Z', type: 'role-response', visible: true };

  it('produces a kind:9 event tagged to the channel with the message as content', () => {
    const e = buildKind9(msg, CHANNEL, stubSigner());
    expect(e.kind).toBe(9);
    expect(e.content).toContain('security is loom #1');
    expect(e.tags).toContainEqual(['h', CHANNEL]);
  });

  it('carries the original Clearing sender as attribution (content prefix, never a forged pubkey)', () => {
    const e = buildKind9(msg, CHANNEL, stubSigner());
    expect(e.content).toMatch(/^\[wren\]/);
    expect(e.pubkey).toBe('ab'.repeat(32)); // the BRIDGE's key, not wren's
  });

  it('carries id + sig from the signer and the correct created_at', () => {
    const e = buildKind9(msg, CHANNEL, stubSigner());
    expect(e.id).toMatch(/^[0-9a-f]{64}$/);
    expect(e.sig).toBe('cd'.repeat(64));
    expect(e.created_at).toBe(Math.floor(Date.parse(msg.ts) / 1000));
  });

  it('is deterministic over the same input (id is a function of the serialization, not random)', () => {
    expect(buildKind9(msg, CHANNEL, stubSigner()).id).toBe(buildKind9(msg, CHANNEL, stubSigner()).id);
  });

  it('refuses to build an unsigned event when the signer is absent', () => {
    expect(() => buildKind9(msg, CHANNEL, null as unknown as NostrSigner)).toThrow(/key|sign/i);
  });
});

describe('#3674 BuzzBridge — the one-way tap, flag + safety', () => {
  const mk = (enabled: boolean, signer: NostrSigner | null = stubSigner()) => {
    const published: unknown[] = [];
    const bridge = new BuzzBridge({ enabled, channel: CHANNEL, signer, publish: async (ev) => { published.push(ev); } });
    return { bridge, published };
  };

  it('OFF by default: a message publishes nothing (flag-gated, safe to ship dark)', async () => {
    const { bridge, published } = mk(false);
    await bridge.onMessage({ from: 'wren', text: 'hi', ts: '2026-07-25T19:00:00Z', type: 'role-response', visible: true });
    expect(published).toEqual([]);
  });

  it('ON: a visible message publishes exactly one signed kind:9', async () => {
    const { bridge, published } = mk(true);
    await bridge.onMessage({ from: 'wren', text: 'hi', ts: '2026-07-25T19:00:00Z', type: 'role-response', visible: true });
    expect(published).toHaveLength(1);
    expect((published[0] as { kind: number }).kind).toBe(9);
  });

  it('ON but invisible (hidden pm-thinking/probe): NOT mirrored — mirror what the room sees', async () => {
    const { bridge, published } = mk(true);
    await bridge.onMessage({ from: 'wren', text: 'internal', ts: '2026-07-25T19:00:00Z', type: 'pm-thinking', visible: false });
    expect(published).toEqual([]);
  });

  it('missing signer with flag ON: refuses loudly, does not publish an unsigned event', async () => {
    const { bridge, published } = mk(true, null);
    await expect(bridge.onMessage({ from: 'wren', text: 'hi', ts: '2026-07-25T19:00:00Z', type: 'role-response', visible: true }))
      .rejects.toThrow(/key|sign/i);
    expect(published).toEqual([]);
  });

  it('a publish failure never throws into the Clearing hot path (best-effort mirror)', async () => {
    const bridge = new BuzzBridge({
      enabled: true, channel: CHANNEL, signer: stubSigner(),
      publish: async () => { throw new Error('relay down'); },
    });
    await expect(bridge.onMessageSafe({ from: 'wren', text: 'hi', ts: '2026-07-25T19:00:00Z', type: 'role-response', visible: true }))
      .resolves.toBeUndefined();
  });
});