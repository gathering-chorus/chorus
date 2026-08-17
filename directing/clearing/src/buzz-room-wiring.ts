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

import os from 'os';
import WebSocket from 'ws';
import type { ClearingMsg, NostrEvent, NostrSigner } from './buzz-bridge';
import { buildRoomIdentity, inboundToClearing, publishToRoom, type RoomIdentity } from './buzz-room';
import { registeredSigner } from './buzz-signer';
import { advanceCursor, defaultCursorPath, readCursor, roomFilter, writeCursor } from './room-replay';
import { emptySeqState, holeMarker, holesFor, recordSeq, seqOf, type SeqState } from './room-sequence';

type Logger = (level: 'info' | 'error', event: string, fields: Record<string, unknown>) => void;

/** How many un-sent messages to hold while the relay is unreachable. */
const MAX_PENDING = 100;

/**
 * #3907 — the Host the relay must see.
 *
 * Exported and pure so it can be asserted directly. The bug lived in the dial
 * ARGUMENTS, and every existing room test injects its own connect(), so nothing
 * that hid inside the default connect could ever be caught by them.
 */
export function dialOptions(relayHost: string | undefined, env: NodeJS.ProcessEnv = process.env): { headers?: { Host: string } } {
  const host = relayHost ?? env.BUZZ_RELAY_HOST_HEADER;
  return host ? { headers: { Host: host } } : {};
}

/** The subscription frame. One definition — it is sent twice (before auth,
 *  optimistically, and again once the relay accepts us).
 *
 *  #3893: the filter comes from the durable cursor, so a reconnect asks for
 *  everything said while we were gone instead of the newest 30. The frame is
 *  built at send time, never cached — a cursor read once at startup would
 *  replay the same window after every disconnect. */
function reqFrame(topic: string, cursor: number | null): string {
  return JSON.stringify(['REQ', 'room', roomFilter(topic, cursor)]);
}

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
  connect?: (url: string, opts?: unknown) => WebSocket;
  /** #3893 — where the replay cursor persists. Injected so a test brings its
   *  own world instead of writing into the running role's ~/.chorus. */
  cursorFile?: string;
  /** #3907 — the Host the relay should see, when we reach it through a tunnel
   *  whose address is not the relay's own. Falls back to BUZZ_RELAY_HOST_HEADER. */
  relayHost?: string;
}


/**
 * The NIP-42 auth event.
 *
 * This is not optional, and no unit test could have told us so: with green
 * tests and no auth the socket connected, published fine, and read back
 * NOTHING — the relay answered every REQ with "auth-required: authenticate
 * before subscribing" and the room sat empty. Found by running it against the
 * live relay (2026-08-11).
 *
 * created_at is computed ONCE and reused. Calling Date.now() again for the sent
 * event lets the two differ by a second across a tick boundary, and the relay
 * then rejects a signature that is in fact correct — failing over a clock while
 * complaining about crypto.
 */
/**
 * #3911 — the URL we DIAL is not the URL we may CLAIM.
 *
 * Reaching the relay through the loopback tunnel means dialing
 * ws://127.0.0.1:13000, and the AUTH event was carrying that address in its
 * relay tag. NIP-42 has the relay validate that tag against its own identity, so
 * every AUTH was refused — `accepted:false`, then "auth-required: authenticate
 * before subscribing", forever. The tunnel fixed the dial and made the AUTH
 * statement false, which is the same mistake as the Host header one layer up.
 */
export function authRelayUrl(dialUrl: string, hostHeader?: string): string {
  if (!hostHeader) return dialUrl;
  const scheme = dialUrl.startsWith('wss://') ? 'wss://' : 'ws://';
  return `${scheme}${hostHeader}`;
}

function buildAuthEvent(signer: NostrSigner, relayUrl: string, challenge: string): NostrEvent {
  const createdAt = Math.floor(Date.now() / 1000);
  const tags = [['relay', relayUrl], ['challenge', challenge]];
  const { id, sig } = signer.signEvent(
    JSON.stringify([0, signer.pubkey, createdAt, 22242, tags, '']),
  );
  return { id, pubkey: signer.pubkey, created_at: createdAt, kind: 22242, tags, content: '', sig };
}

interface RoomState {
  deps: RoomWiringDeps;
  identity: RoomIdentity;
  connSigner: NostrSigner;
  seenIds: Set<string>;
  pending: NostrEvent[];
  ws: WebSocket | null;
  stopped: boolean;
  authed: boolean;
  authEventId: string | null;
  /** #3893 — newest rendered note's created_at; the replay point. */
  cursor: number | null;
  cursorFile: string;
  /** #3907 — options handed to every dial, including reconnects. */
  dialOpts: { headers?: { Host: string } };
  /** #3893 — per-author counters, so a message that never came can be named. */
  seq: SeqState;
  /** Holes already announced, so the marker is said once and not on every note. */
  announced: Map<string, string>;
}

/** Remember an id we published, bounded — an unbounded set in a process that
 *  runs for weeks is a slow leak, and the echo window only needs to cover the
 *  round trip. */
function rememberSent(st: RoomState, id: string): void {
  st.seenIds.add(id);
  if (st.seenIds.size > 500) {
    const oldest = st.seenIds.values().next().value;
    if (oldest !== undefined) st.seenIds.delete(oldest);
  }
}

/** Render an inbound note, or log WHY it was not rendered. A silent drop is
 *  indistinguishable from a relay that never delivered — the debugging hole
 *  that cost an afternoon on 2026-08-11. */
function handleInbound(st: RoomState, ev: NostrEvent): void {
  const { msg, disposition } = inboundToClearing(ev, st.identity, st.seenIds);
  if (msg) {
    st.deps.ingest(msg);
    // #3893 — advance ONLY on a rendered note. Advancing on one we dropped
    // would move the replay point past a message nobody ever saw, which is the
    // silent loss this cursor exists to end.
    const next = advanceCursor(st.cursor, ev.created_at);
    if (next !== st.cursor) {
      st.cursor = next;
      if (next !== null) writeCursor(st.cursorFile, next);
    }
    announceHoles(st, msg.from, ev);
    return;
  }
  if (disposition !== 'own-echo') {
    st.deps.log('info', 'buzz.room.not_rendered', { disposition, pubkey: ev.pubkey.slice(0, 8) });
  }
}

/**
 * #3893 — say what never arrived, in the room, once.
 *
 * The marker is ingested as a message rather than logged, because a hole Jeff
 * cannot see is the defect itself: he spent days answering messages one, four
 * and eight without knowing two, three, five, six and seven existed. It is
 * announced once per distinct hole set — repeating it under every subsequent
 * note would train him to scroll past it, which is the same as hiding it.
 */
function announceHoles(st: RoomState, author: string, ev: NostrEvent): void {
  recordSeq(st.seq, author, seqOf(ev));
  const missing = holesFor(st.seq, author);
  const marker = holeMarker(author, missing);
  if (marker === (st.announced.get(author) ?? '')) return;
  st.announced.set(author, marker);
  if (marker === '') return;   // the hole closed — a late note arrived
  st.deps.log('error', 'buzz.room.gap', { author, missing });
  st.deps.ingest({
    from: 'system',
    text: marker,
    ts: new Date().toISOString(),
    type: 'gap',
    visible: true,
  });
}

function onAuthChallenge(st: RoomState, challenge: string): void {
  // Claim the relay's own address, not the tunnel we happen to reach it through.
  const claimed = authRelayUrl(st.deps.relayUrl, st.deps.relayHost ?? process.env.BUZZ_RELAY_HOST_HEADER);
  const auth = buildAuthEvent(st.connSigner, claimed, challenge);
  st.authEventId = auth.id;
  st.ws?.send(JSON.stringify(['AUTH', auth]));
  st.deps.log('info', 'buzz.room.authenticating', { as: st.deps.authAs ?? 'wren' });
}

function onAuthAccepted(st: RoomState, accepted: unknown): void {
  st.authed = true;
  st.deps.log('info', 'buzz.room.authenticated', { accepted });
  st.ws?.send(reqFrame(st.deps.topic, st.cursor));
  for (const ev of st.pending.splice(0)) st.ws?.send(JSON.stringify(['EVENT', ev]));
}

/** Dispatch one relay frame. */
function onFrame(st: RoomState, raw: unknown): void {
  let m: unknown[];
  try {
    m = JSON.parse(String(raw)) as unknown[];
  } catch {
    return;
  }
  if (!Array.isArray(m)) return;
  const kind = m[0];
  if (kind === 'AUTH' && typeof m[1] === 'string') return onAuthChallenge(st, m[1]);
  if (kind === 'OK' && m[1] === st.authEventId && !st.authed) return onAuthAccepted(st, m[2]);
  if (kind === 'EVENT' && m[2] && typeof m[2] === 'object') return handleInbound(st, m[2] as NostrEvent);
  if (kind === 'NOTICE') st.deps.log('error', 'buzz.room.notice', { notice: String(m[1]) });
}

/** Queue an event the socket cannot send yet, bounded. Drops the OLDEST during
 *  an outage — the newest messages are the ones still worth saying when the
 *  relay returns — and says so, because a silently shortened backlog is how a
 *  room quietly loses history. */
function queuePending(st: RoomState, ev: NostrEvent): void {
  st.pending.push(ev);
  if (st.pending.length > MAX_PENDING) {
    const dropped = st.pending.splice(0, st.pending.length - MAX_PENDING);
    st.deps.log('error', 'buzz.room.backlog_dropped', { dropped: dropped.length, kept: MAX_PENDING });
  }
}

function connectRoom(st: RoomState, connect: (url: string, opts?: unknown) => WebSocket): void {
  if (st.stopped) return;
  // A reconnect starts a fresh session: the relay challenges again, and carrying
  // `authed` across would skip the re-auth and subscribe to a socket that
  // refuses us — the room would go quiet after one blip.
  st.authed = false;
  st.authEventId = null;
  // Options go on EVERY dial, not just the first — a reconnect that drops the
  // Host would silently resubscribe to nothing.
  const ws = connect(st.deps.relayUrl, st.dialOpts);
  st.ws = ws;
  ws.on('open', () => {
    st.deps.log('info', 'buzz.room.connected', { relay: st.deps.relayUrl, topic: st.deps.topic });
    // Subscribe optimistically: a relay that demands auth first refuses this and
    // we re-send after the AUTH ok; one that does not never makes us wait for a
    // challenge that will not come.
    ws.send(reqFrame(st.deps.topic, st.cursor));
  });
  ws.on('message', (raw: unknown) => onFrame(st, raw));
  ws.on('close', () => {
    if (st.stopped) return;
    st.deps.log('error', 'buzz.room.disconnected', { retryMs: 5000 });
    setTimeout(() => connectRoom(st, connect), 5000).unref();
  });
  ws.on('error', (e: Error) => st.deps.log('error', 'buzz.room.socket_error', { reason: e.message }));
}

export function startRoom(deps: RoomWiringDeps): RoomWiring {
  const st: RoomState = {
    deps,
    identity: deps.identity ?? buildRoomIdentity(),
    // #3910 — the socket authenticates as the bridge service identity, whose
    // pubkey IS in the relay allowlist because the graph registered it.
    connSigner: registeredSigner(deps.authAs ?? 'bridge'),
    seenIds: new Set<string>(),
    pending: [],
    ws: null,
    stopped: false,
    authed: false,
    authEventId: null,
    cursor: null,
    seq: emptySeqState(),
    announced: new Map<string, string>(),
    cursorFile: deps.cursorFile ?? defaultCursorPath(os.homedir(), deps.topic),
    dialOpts: {},
  };
  st.cursor = readCursor(st.cursorFile);
  // #3907 — the relay virtual-hosts by Host. Reaching it through the loopback
  // tunnel (127.0.0.1:<port>) sends a Host of 127.0.0.1 and the relay answers
  // 404 — the socket opens, the subscription never does, and the room looks
  // simply quiet. Found live by Kade; a unit test could not see it because the
  // stub socket never had a Host to get wrong.
  const dialOpts = dialOptions(deps.relayHost);
  const connect = deps.connect ?? ((url: string, opts?: unknown) => new WebSocket(url, opts as never));
  st.dialOpts = dialOpts;
  connectRoom(st, connect);

  return {
    enabled: true,
    publish: (msg: ClearingMsg) => {
      void publishToRoom(msg, {
        topic: deps.topic,
        identity: st.identity,
        publish: (ev) => {
          rememberSent(st, ev.id);
          if (st.ws && st.ws.readyState === WebSocket.OPEN) st.ws.send(JSON.stringify(['EVENT', ev]));
          else queuePending(st, ev);
          return Promise.resolve();
        },
        log: deps.log,
      }).catch((err: unknown) => {
        deps.log('error', 'buzz.room.publish_failed', {
          reason: err instanceof Error ? err.message : String(err),
        });
      });
    },
    stop: () => {
      st.stopped = true;
      try { st.ws?.close(); } catch { /* already gone */ }
    },
  };
}
