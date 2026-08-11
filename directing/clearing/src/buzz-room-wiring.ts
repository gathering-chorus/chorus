/**
 * #3823 — live wiring for the room: one long-lived socket, both directions.
 *
 * Separate from buzz-wiring.ts on purpose. That one is the #3696 spike's
 * one-way mirror signing as a single bridge key; this is the test-drive's
 * per-actor room. They will not both survive — when Jeff has used this, one of
 * them gets deleted rather than left around to confuse the next reader.
 *
 * Everything here is best-effort. A relay outage must degrade the room to
 * exactly what it is today, never break it: the Clearing has to keep working
 * when Bedroom is off.
 */

import WebSocket from 'ws';
import type { NostrEvent } from './buzz-bridge';
import type { ClearingMsg } from './buzz-bridge';
import { buildRoomIdentity, inboundToClearing, publishToRoom, type RoomIdentity } from './buzz-room';
import { derivedSigner } from './buzz-signer';

type Logger = (level: 'info' | 'error', event: string, fields: Record<string, unknown>) => void;

/** How many un-sent messages to hold while the relay is unreachable. */
const MAX_PENDING = 100;

export interface RoomWiring {
  enabled: boolean;
  /** Call for every Clearing message; publishes it to the room as its author. */
  publish: (msg: ClearingMsg) => void;
  stop: () => void;
}

export interface RoomWiringDeps {
  relayUrl: string;
  topic: string;
  identity?: RoomIdentity;
  /** Hand an inbound message to the Clearing. */
  ingest: (msg: ClearingMsg) => void;
  log: Logger;
  /** Which actor this Clearing authenticates AS on the socket. The connection
   *  identity is separate from per-message authorship: messages are signed by
   *  their author, the socket is authenticated by whoever is running it. */
  authAs?: string;
  connect?: (url: string) => WebSocket;
}

export function startRoom(deps: RoomWiringDeps): RoomWiring {
  const identity = deps.identity ?? buildRoomIdentity();
  const connect = deps.connect ?? ((url: string) => new WebSocket(url));
  // Ids we published. Bounded — an unbounded set in a long-lived process is a
  // slow leak, and the echo window only needs to cover the round trip.
  const seenIds = new Set<string>();
  const rememberSent = (id: string): void => {
    seenIds.add(id);
    if (seenIds.size > 500) {
      const oldest = seenIds.values().next().value;
      if (oldest !== undefined) seenIds.delete(oldest);
    }
  };

  let ws: WebSocket | null = null;
  let stopped = false;
  let authed = false;
  let authEventId: string | null = null;
  const connSigner = derivedSigner(deps.authAs ?? 'wren');
  const pending: NostrEvent[] = [];

  const open = (): void => {
    if (stopped) return;
    // A reconnect starts a fresh session: the relay will challenge again, and
    // carrying `authed` across would make us skip the re-auth and subscribe to
    // a socket that refuses us — the room would go quiet after one blip.
    authed = false;
    authEventId = null;
    ws = connect(deps.relayUrl);

    ws.on('open', () => {
      deps.log('info', 'buzz.room.connected', { relay: deps.relayUrl, topic: deps.topic });
      // Subscribe optimistically. A relay that demands auth first refuses this
      // and we re-send after the AUTH ok; a relay that does not never makes us
      // wait for a challenge that will not come.
      ws?.send(JSON.stringify(['REQ', 'room', { kinds: [1], '#t': [deps.topic], limit: 30 }]));
    });

    ws.on('message', (raw: unknown) => {
      let m: unknown[];
      try {
        m = JSON.parse(String(raw));
      } catch {
        return;
      }
      if (!Array.isArray(m)) return;

      if (m[0] === 'AUTH' && typeof m[1] === 'string') {
        // NIP-42. This is not optional and the unit tests could not have told
        // us so: with green tests and no auth, the socket connected, published
        // fine, and read back NOTHING — the relay answered every REQ with
        // "auth-required: authenticate before subscribing" and the room sat
        // empty. Found by running it against the live relay (2026-08-11).
        // created_at is computed ONCE. Calling Date.now() again for the sent
        // event would let the two differ by a second across a tick boundary,
        // and the relay would reject a signature that is in fact correct —
        // over a clock, while complaining about crypto.
        const createdAt = Math.floor(Date.now() / 1000);
        const tags = [['relay', deps.relayUrl], ['challenge', m[1]]];
        const auth = connSigner.signEvent(
          JSON.stringify([0, connSigner.pubkey, createdAt, 22242, tags, '']),
        );
        authEventId = auth.id;
        ws?.send(JSON.stringify(['AUTH', {
          id: auth.id, pubkey: connSigner.pubkey, created_at: createdAt,
          kind: 22242, tags, content: '', sig: auth.sig,
        }]));
        deps.log('info', 'buzz.room.authenticating', { as: deps.authAs ?? 'wren' });
        return;
      }
      if (m[0] === 'OK' && m[1] === authEventId && !authed) {
        authed = true;
        deps.log('info', 'buzz.room.authenticated', { accepted: m[2] });
        ws?.send(JSON.stringify(['REQ', 'room', { kinds: [1], '#t': [deps.topic], limit: 30 }]));
        for (const ev of pending.splice(0)) ws?.send(JSON.stringify(['EVENT', ev]));
        return;
      }
      if (m[0] === 'EVENT' && m[2] && typeof m[2] === 'object') {
        const ev = m[2] as NostrEvent;
        const { msg, disposition } = inboundToClearing(ev, identity, seenIds);
        if (msg) {
          deps.ingest(msg);
          return;
        }
        // Log every refusal WITH its state. A silent drop here is
        // indistinguishable from a relay that never delivered, which is the
        // debugging hole that ate this morning.
        if (disposition !== 'own-echo') {
          deps.log('info', 'buzz.room.not_rendered', { disposition, pubkey: ev.pubkey?.slice(0, 8) });
        }
        return;
      }
      if (m[0] === 'NOTICE') deps.log('error', 'buzz.room.notice', { notice: String(m[1]) });
    });

    ws.on('close', () => {
      if (stopped) return;
      deps.log('error', 'buzz.room.disconnected', { retryMs: 5000 });
      setTimeout(open, 5000).unref?.();
    });
    ws.on('error', (e: Error) => deps.log('error', 'buzz.room.socket_error', { reason: e.message }));
  };

  open();

  return {
    enabled: true,
    publish: (msg: ClearingMsg) => {
      void publishToRoom(msg, {
        topic: deps.topic,
        identity,
        publish: async (ev) => {
          rememberSent(ev.id);
          if (ws && ws.readyState === WebSocket.OPEN) {
            ws.send(JSON.stringify(['EVENT', ev]));
            return;
          }
          // Bounded (Silas, #3823 review). An unbounded queue turns a Bedroom
          // outage into a slow leak in a process that runs for weeks. Drop the
          // OLDEST — during an outage the newest messages are the ones still
          // worth saying when the relay comes back — and say so, because a
          // silently shortened backlog is how a room quietly loses history.
          pending.push(ev);
          if (pending.length > MAX_PENDING) {
            const dropped = pending.splice(0, pending.length - MAX_PENDING);
            deps.log('error', 'buzz.room.backlog_dropped', { dropped: dropped.length, kept: MAX_PENDING });
          }
        },
        log: deps.log,
      }).catch((err: unknown) => {
        deps.log('error', 'buzz.room.publish_failed', {
          reason: err instanceof Error ? err.message : String(err),
        });
      });
    },
    stop: () => {
      stopped = true;
      try { ws?.close(); } catch { /* already gone */ }
    },
  };
}
