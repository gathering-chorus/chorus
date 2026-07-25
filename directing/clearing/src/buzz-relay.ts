// @test-type: unit — frame construction is pure + injected-socket tested; the live
// WebSocket path is integration-proven against the relay, not in jest.
/**
 * #3696 — the live Buzz relay adapter for the Clearing bridge. Owns the WebSocket
 * to our self-hosted relay: connect, (optional) NIP-42 AUTH, and publish a signed
 * event, resolving on the relay's ['OK', id, accepted, msg] verdict.
 *
 * The `#team` NIP-29 channel must EXIST before kind:9 lands (create-then-mirror —
 * a one-time `buzz-admin` setup step, not the bridge's job). Config: relay URL +
 * team channel id come from env (BUZZ_RELAY_URL, BUZZ_TEAM_CHANNEL). The relay is
 * host-scoped — connect via the LAN IP (BUZZ_DOMAIN), never localhost.
 *
 * Kept separate from buzz-bridge.ts (pure logic) and buzz-signer.ts (@noble crypto)
 * so each boundary is testable alone. Here the frame construction is pure; the live
 * socket is proven end-to-end against the relay.
 */
import type { NostrEvent, NostrSigner } from './buzz-bridge';

/** Build the wire frame for publishing an event: NIP-01 ['EVENT', <event>]. */
export function eventFrame(ev: NostrEvent): string {
  return JSON.stringify(['EVENT', ev]);
}

/** Build a NIP-42 AUTH response: sign the relay's challenge as a kind:22242 event. */
export function authFrame(challenge: string, relayUrl: string, signer: NostrSigner, nowSec: number): string {
  const tags = [['relay', relayUrl], ['challenge', challenge]];
  const serialized = JSON.stringify([0, signer.pubkey, nowSec, 22242, tags, '']);
  const { id, sig } = signer.signEvent(serialized);
  const ev: NostrEvent = { id, pubkey: signer.pubkey, created_at: nowSec, kind: 22242, tags, content: '', sig };
  return JSON.stringify(['AUTH', ev]);
}

/** The relay verdict for a published event. */
export interface PublishResult {
  ok: boolean;
  id: string;
  message: string;
}

/** Parse a relay message: ['OK', id, accepted, msg] | ['AUTH', challenge] | other. */
export function parseRelayMsg(raw: string): { type: string; payload: unknown[] } {
  try {
    const m = JSON.parse(raw);
    return Array.isArray(m) ? { type: String(m[0]), payload: m } : { type: 'unknown', payload: [] };
  } catch {
    return { type: 'unparseable', payload: [] };
  }
}

/** Minimal WebSocket surface — the live `ws` client satisfies it; tests inject a stub. */
export interface RelaySocket {
  send(data: string): void;
  on(event: 'open' | 'message' | 'error' | 'close', cb: (arg?: unknown) => void): void;
  close(): void;
}

export interface RelayConnDeps {
  relayUrl: string;
  signer: NostrSigner;
  connect: (url: string) => RelaySocket;
  nowSec: () => number;
  timeoutMs?: number;
  /** Grace after open to wait for a NIP-42 AUTH challenge before sending on an
   *  OPEN relay (which never challenges). Observed live: restricted relays
   *  challenge proactively on connect, so auth-first is the correct order. */
  authGraceMs?: number;
}

/**
 * Publish one signed event, handling a NIP-42 AUTH challenge inline. Resolves on
 * the ['OK', id, accepted, msg] for our event; rejects on timeout or a non-OK.
 * Best-effort: the caller (bridge) swallows a rejection so the Clearing never breaks.
 */
export function publishEvent(deps: RelayConnDeps, ev: NostrEvent): Promise<PublishResult> {
  const timeoutMs = deps.timeoutMs ?? 8000;
  const authGraceMs = deps.authGraceMs ?? 600;
  return new Promise<PublishResult>((resolve, reject) => {
    const ws = deps.connect(deps.relayUrl);
    let settled = false;
    let eventSent = false;
    const done = (fn: () => void) => { if (!settled) { settled = true; try { ws.close(); } catch { /* noop */ } fn(); } };
    const timer = setTimeout(() => done(() => reject(new Error('buzz-relay: no OK within timeout'))), timeoutMs);
    const sendEvent = () => { if (!eventSent) { eventSent = true; ws.send(eventFrame(ev)); } };
    let grace: ReturnType<typeof setTimeout> | undefined;
    // AUTH-FIRST (observed live): a restricted relay challenges proactively on
    // connect — auth, THEN send the EVENT (single WS = in-order, relay processes
    // auth before event). An OPEN relay never challenges, so a grace timer sends
    // the EVENT if no challenge arrives.
    ws.on('open', () => { grace = setTimeout(sendEvent, authGraceMs); });
    ws.on('message', (raw) => {
      const { type, payload } = parseRelayMsg(String(raw));
      if (type === 'AUTH' && typeof payload[1] === 'string') {
        if (grace) clearTimeout(grace);
        ws.send(authFrame(payload[1], deps.relayUrl, deps.signer, deps.nowSec()));
        sendEvent(); // in-order after the AUTH frame
      } else if (type === 'OK' && payload[1] === ev.id) {
        clearTimeout(timer);
        done(() => resolve({ ok: payload[2] === true, id: ev.id, message: String(payload[3] ?? '') }));
      }
      // an OK for the AUTH event's own id (≠ ev.id) is the auth ack — ignored here.
    });
    ws.on('error', (e) => { clearTimeout(timer); done(() => reject(e instanceof Error ? e : new Error(String(e)))); });
  });
}
