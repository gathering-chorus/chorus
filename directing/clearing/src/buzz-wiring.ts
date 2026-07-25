/**
 * #3696 — Clearing→Buzz wiring (Leg A). Assembles the flag-gated bridge from env
 * and returns a single `mirror(msg)` the server calls on every message. All I/O
 * is best-effort: a relay outage or a missing key can never break the Clearing.
 *
 * Env:
 *   BUZZ_BRIDGE_ENABLED=1        — flag (default OFF; ships dark)
 *   BUZZ_RELAY_URL               — ws://<lan-ip>:3000 (host-scoped; NOT localhost)
 *   BUZZ_TEAM_CHANNEL            — the #team NIP-29 group id (created via buzz-admin)
 *   BUZZ_BRIDGE_NOSTR_KEY        — principal-bridge's 32-byte hex key (#3618/#3695)
 *
 * Per-actor signing (each role/human signs as their own Principal-bound key) rides
 * #3695; until those land, the bridge signs as `principal-bridge` and preserves the
 * true author in content ([from] text) — never a forged pubkey.
 */
import WebSocket from 'ws';
import { BuzzBridge, type ClearingMsg, type NostrEvent } from './buzz-bridge';
import { nobleSigner } from './buzz-signer';
import { publishEvent, type RelaySocket } from './buzz-relay';

export interface BuzzWiring {
  enabled: boolean;
  mirror: (msg: ClearingMsg) => void;
}

type Logger = (level: 'info' | 'error', event: string, fields: Record<string, unknown>) => void;

export function buildBuzzWiring(env: NodeJS.ProcessEnv, log: Logger): BuzzWiring {
  const enabled = env.BUZZ_BRIDGE_ENABLED === '1';
  if (!enabled) {
    log('info', 'buzz.bridge.disabled', {});
    return { enabled: false, mirror: () => { /* dark */ } };
  }
  const relayUrl = env.BUZZ_RELAY_URL ?? '';
  const channel = env.BUZZ_TEAM_CHANNEL ?? '';
  const keyHex = env.BUZZ_BRIDGE_NOSTR_KEY ?? '';
  if (!relayUrl || !channel || !keyHex) {
    log('error', 'buzz.bridge.misconfig', { hasRelay: !!relayUrl, hasChannel: !!channel, hasKey: !!keyHex });
    return { enabled: false, mirror: () => { /* refuse to run half-configured */ } };
  }

  const signer = nobleSigner(keyHex);
  const bridge = new BuzzBridge({
    enabled: true,
    channel,
    signer,
    publish: (ev: NostrEvent) =>
      publishEvent(
        { relayUrl, signer, connect: (url) => new WebSocket(url) as unknown as RelaySocket, nowSec: () => Math.floor(Date.now() / 1000) },
        ev,
      ).then((r) => {
        if (!r.ok) throw new Error(`relay rejected: ${r.message}`);
        log('info', 'buzz.bridge.published', { id: r.id });
      }),
    log,
  });

  log('info', 'buzz.bridge.enabled', { relayUrl, channel: channel.slice(0, 8) });
  return {
    enabled: true,
    // fire-and-forget; onMessageSafe swallows every error path
    mirror: (msg: ClearingMsg) => { void bridge.onMessageSafe(msg); },
  };
}
