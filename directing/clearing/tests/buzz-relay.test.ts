// @test-type: unit — injected stubs (signer/socket/env); no live relay, no real key, brings its own world.
// #3696 — the live relay adapter's pure/framing logic + the publish state machine
// with an injected socket stub. The real WebSocket is integration-proven against
// the relay (a bad sig / missing channel is rejected there); here we pin framing,
// the OK verdict, the NIP-42 AUTH inline handshake, and timeout — all hermetic.
import { eventFrame, authFrame, parseRelayMsg, publishEvent, type RelaySocket } from '../src/buzz-relay';
import type { NostrEvent, NostrSigner } from '../src/buzz-bridge';

const signer: NostrSigner = {
  pubkey: 'ab'.repeat(32),
  signEvent: () => ({ id: 'ff'.repeat(32), sig: 'cd'.repeat(64) }),
};
const ev: NostrEvent = {
  id: '11'.repeat(32), pubkey: 'ab'.repeat(32), created_at: 1000, kind: 9,
  tags: [['h', 'team']], content: '[wren] hi', sig: 'cd'.repeat(64),
};

describe('#3696 relay framing (pure)', () => {
  it('eventFrame is NIP-01 ["EVENT", event]', () => {
    expect(JSON.parse(eventFrame(ev))).toEqual(['EVENT', ev]);
  });
  it('authFrame signs the challenge as a kind:22242 with relay + challenge tags', () => {
    const f = JSON.parse(authFrame('chal-xyz', 'ws://r:3000', signer, 1234));
    expect(f[0]).toBe('AUTH');
    expect(f[1].kind).toBe(22242);
    expect(f[1].tags).toContainEqual(['relay', 'ws://r:3000']);
    expect(f[1].tags).toContainEqual(['challenge', 'chal-xyz']);
    expect(f[1].sig).toBe('cd'.repeat(64));
  });
  it('parseRelayMsg classifies OK/AUTH and survives garbage', () => {
    expect(parseRelayMsg('["OK","id",true,""]').type).toBe('OK');
    expect(parseRelayMsg('["AUTH","challenge"]').type).toBe('AUTH');
    expect(parseRelayMsg('not json').type).toBe('unparseable');
  });
});

// scriptable socket stub: onOpen fires after connect (relay can challenge
// proactively); onSend fires per client frame (relay ACKs). Matches the observed
// live flow — auth challenge on connect, then verdicts on the sent frames.
function stubSocket(opts: {
  onOpen?: (emit: (raw: string) => void) => void;
  onSend?: (raw: string, emit: (raw: string) => void, sent: string[]) => void;
}): (url: string) => RelaySocket {
  return () => {
    const handlers: Record<string, (a?: unknown) => void> = {};
    const sent: string[] = [];
    const emit = (raw: string) => handlers['message']?.(raw);
    return {
      send: (d: string) => { sent.push(d); opts.onSend?.(d, emit, sent); },
      on: (e, cb) => { handlers[e] = cb; if (e === 'open') queueMicrotask(() => { cb(); opts.onOpen?.(emit); }); },
      close: () => { /* noop */ },
    };
  };
}

describe('#3696 publishEvent — auth-first flow, verdict, timeout', () => {
  it('OPEN relay (no challenge): grace fires, EVENT sent, ok=true', async () => {
    const connect = stubSocket({ onSend: (_d, emit, sent) => { if (sent.length === 1) emit(JSON.stringify(['OK', ev.id, true, ''])); } });
    const r = await publishEvent({ relayUrl: 'ws://r:3000', signer, connect, nowSec: () => 1, authGraceMs: 5 }, ev);
    expect(r).toEqual({ ok: true, id: ev.id, message: '' });
  });

  it('RESTRICTED relay: AUTH challenge on connect → auth → EVENT → ok=true (never sends before auth)', async () => {
    const connect = stubSocket({
      onOpen: (emit) => emit(JSON.stringify(['AUTH', 'challenge-1'])),           // proactive challenge
      onSend: (_d, emit, sent) => {
        if (sent.length === 1) emit(JSON.stringify(['OK', 'authEvtId', true, ''])); // ack the AUTH (≠ ev.id, ignored)
        if (sent.length === 2) emit(JSON.stringify(['OK', ev.id, true, '']));       // ack the EVENT
      },
    });
    const r = await publishEvent({ relayUrl: 'ws://r:3000', signer, connect, nowSec: () => 1, authGraceMs: 500 }, ev);
    expect(r.ok).toBe(true);
  });

  it('surfaces a relay rejection as ok=false with the reason', async () => {
    const connect = stubSocket({ onSend: (_d, emit, sent) => { if (sent.length === 1) emit(JSON.stringify(['OK', ev.id, false, 'blocked: channel must exist'])); } });
    const r = await publishEvent({ relayUrl: 'ws://r:3000', signer, connect, nowSec: () => 1, authGraceMs: 5 }, ev);
    expect(r.ok).toBe(false);
    expect(r.message).toMatch(/channel must exist/);
  });

  it('rejects on timeout when no OK arrives', async () => {
    const connect = stubSocket({});
    await expect(publishEvent({ relayUrl: 'ws://r:3000', signer, connect, nowSec: () => 1, timeoutMs: 40, authGraceMs: 5 }, ev))
      .rejects.toThrow(/timeout/);
  });
});