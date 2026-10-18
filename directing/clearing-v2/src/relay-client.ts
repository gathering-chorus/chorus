// @domain: messages

import { NostrEvent } from './types';
import { buildEventId, serializeEvent } from './identity';

/** The room's reader key (Silas 2026-10-15 13:41: the bridge signer v1 registered,
 *  reads only, not a member). Key material stays outside the room: the caller
 *  injects a signer, never a key. */
export interface RelaySigner {
  pubkey: string;
  /** Schnorr signature (hex) over the 32-byte event id (hex). */
  sign(idHex: string): string;
}

/** NIP-42: answer the relay's challenge with a signed kind 22242 event. Same frame as
 *  v1's buzz-relay.ts authFrame. */
export function authFrame(challenge: string, relayUrl: string, signer: RelaySigner, nowSec: number): string {
  const tags = [['relay', relayUrl], ['challenge', challenge]];
  const id = buildEventId(JSON.stringify([0, signer.pubkey, nowSec, 22242, tags, '']));
  return JSON.stringify(['AUTH', { id, pubkey: signer.pubkey, created_at: nowSec, kind: 22242, tags, content: '', sig: signer.sign(id) }]);
}

export interface WebSocketLike {
  send(data: string): void;
  close(): void;
  on(event: 'open' | 'message' | 'close' | 'error', listener: (...args: unknown[]) => void): void;
}

export class MockWebSocket implements WebSocketLike {
  public sentMessages: string[] = [];
  private listeners: Map<string, Array<(...args: unknown[]) => void>> = new Map();

  on(event: 'open' | 'message' | 'close' | 'error', listener: (...args: unknown[]) => void): void {
    if (!this.listeners.has(event)) {
      this.listeners.set(event, []);
    }
    this.listeners.get(event)!.push(listener);
  }

  send(data: string): void {
    this.sentMessages.push(data);
  }

  close(): void {
    this.emit('close');
  }

  triggerClose(): void {
    this.emit('close');
  }

  triggerOpen(): void {
    this.emit('open');
  }

  triggerMessage(data: string): void {
    this.emit('message', data);
  }

  triggerError(err: Error): void {
    this.emit('error', err);
  }

  private emit(event: string, ...args: unknown[]): void {
    const handlers = this.listeners.get(event) || [];
    for (const h of handlers) {
      h(...args);
    }
  }
}

export interface BuzzRelayClientOptions {
  relayUrl: string;
  topic?: string;
  wsFactory?: () => WebSocketLike;
  onEvent?: (event: NostrEvent) => void;
  subId?: string;
  autoReconnect?: boolean;
  /** NIP-42 reader key; without it a challenging relay never lets the room read. */
  signer?: RelaySigner;
  /** The relay is host-scoped (v1: "connect via the LAN IP, never localhost"). */
  hostHeader?: string;
  nowSec?: () => number;
  /** #4571 — wait before redialing a dropped socket (default 5 s). */
  reconnectDelayMs?: number;
}

export class BuzzRelayClient {
  private relayUrl: string;
  private topic: string;
  private wsFactory?: () => WebSocketLike;
  private onEventCallback?: (event: NostrEvent) => void;
  private subId: string;
  private ws: WebSocketLike | null = null;
  private connected = false;
  private buffer: NostrEvent[] = [];
  private signer?: RelaySigner;
  private hostHeader?: string;
  private nowSec: () => number;
  private authState: 'none' | 'challenged' | 'sent' | 'ok' | 'refused' | 'no-signer' = 'none';
  private authEventId = '';
  private reconnectDelayMs: number;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private stopped = false;
  /** #4592 — one-shot reads (today's notes for the counts page), kept apart from the room's feed. */
  private fetches = new Map<string, { events: NostrEvent[]; done: (events: NostrEvent[]) => void }>();
  private fetchSeq = 0;
  /** #4608 — publishes waiting for the relay's OK, by event id. */
  private pendingOks = new Map<string, (r: { ok: boolean; reason: string }) => void>();

  constructor(options: BuzzRelayClientOptions) {
    this.relayUrl = options.relayUrl;
    this.topic = options.topic || 'team';
    this.wsFactory = options.wsFactory;
    this.onEventCallback = options.onEvent;
    this.subId = options.subId || 'team-sub';
    this.signer = options.signer;
    this.hostHeader = options.hostHeader;
    this.nowSec = options.nowSec || (() => Math.floor(Date.now() / 1000));
    this.reconnectDelayMs = options.reconnectDelayMs ?? 5000;
  }

  /** 'no-signer' = the relay asked for AUTH and the room has no reader key: name it. */
  getAuthState(): string {
    return this.authState;
  }

  getRelayUrl(): string {
    return this.relayUrl;
  }

  getTopic(): string {
    return this.topic;
  }

  connect(): void {
    this.stopped = false;
    // a new socket starts unauthenticated: a reconnect must not inherit the last AUTH
    this.authState = 'none';
    this.authEventId = '';
    if (!this.relayUrl) {
      this.connected = false;
      return;
    }
    if (this.wsFactory) {
      this.ws = this.wsFactory();
    } else {
      // Lazy load 'ws' when running in Node.js
      const WebSocket = require('ws');
      this.ws = this.hostHeader ? new WebSocket(this.relayUrl, { headers: { Host: this.hostHeader } }) : new WebSocket(this.relayUrl);
    }

    const ws = this.ws;
    if (!ws) return;

    ws.on('open', () => {
      this.connected = true;
      this.subscribe();
      this.flushBuffer();
    });

    ws.on('message', (data: unknown) => {
      this.handleMessage(String(data));
    });

    ws.on('close', () => {
      this.connected = false;
      this.ws = null;
      this.scheduleReconnect();
    });

    ws.on('error', () => {
      this.connected = false;
    });
  }

  /** #4571 — the room can read: the socket is open and, if the relay challenged,
   *  it accepted our AUTH. An open socket the relay refuses is not "connected". */
  isConnected(): boolean {
    return this.connected && (this.authState === 'none' || this.authState === 'ok');
  }

  private scheduleReconnect(): void {
    if (this.stopped || this.reconnectTimer) return;
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      if (!this.stopped) this.connect();
    }, this.reconnectDelayMs);
    (this.reconnectTimer as { unref?: () => void }).unref?.();
  }

  /** The relay's own address for the AUTH relay tag (v1 authRelayUrl): dialing through
   *  the tunnel, the relay only accepts a tag naming the Host it serves. */
  private authRelayTag(): string {
    if (!this.hostHeader) return this.relayUrl;
    return `${this.relayUrl.startsWith('wss://') ? 'wss://' : 'ws://'}${this.hostHeader}`;
  }

  getBufferedCount(): number {
    return this.buffer.length;
  }

  publish(event: NostrEvent): boolean {
    if (this.connected && this.ws) {
      this.ws.send(JSON.stringify(['EVENT', event]));
      return true;
    }
    this.buffer.push(event);
    return true;
  }

  private subscribe(): void {
    if (!this.ws) return;
    const filter = {
      kinds: [1],
      '#t': [this.topic, 'reply'],
      limit: 200,
    };
    this.ws.send(JSON.stringify(['REQ', this.subId, filter]));
  }

  private flushBuffer(): void {
    if (!this.connected || !this.ws) return;
    while (this.buffer.length > 0) {
      const event = this.buffer.shift();
      if (event) {
        this.ws.send(JSON.stringify(['EVENT', event]));
      }
    }
  }

  private handleMessage(raw: string): void {
    let parsed: unknown;
    try {
      parsed = JSON.parse(raw);
    } catch {
      return; // ignore unparseable
    }
    if (!Array.isArray(parsed)) return;
    if (parsed[0] === 'AUTH' && typeof parsed[1] === 'string') {
      this.answerChallenge(parsed[1]);
    } else if (parsed[0] === 'OK' && this.authEventId && parsed[1] === this.authEventId) {
      this.authState = parsed[2] === true ? 'ok' : 'refused';
    } else if (this.handleOkFrame(parsed) || this.handleFetchFrame(parsed)) {
      // a one-shot read's frame (#4592); never the room's feed
    } else if (parsed[0] === 'EVENT' && parsed[2] && this.onEventCallback) {
      this.onEventCallback(parsed[2] as NostrEvent);
    }
  }

  /**
   * #4592 — every #team note since `since` (epoch seconds), as one read that ends
   * at the relay's EOSE. Rejects, naming why, if the relay is not readable or
   * never finishes: an empty list must only ever mean nothing was there.
   */
  fetchSince(since: number, timeoutMs = 10000): Promise<NostrEvent[]> {
    if (!this.ws || !this.isConnected()) return Promise.reject(new Error('relay not connected'));
    const id = `fetch-${++this.fetchSeq}`;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.fetches.delete(id);
        if (this.ws) this.ws.send(JSON.stringify(['CLOSE', id]));
        reject(new Error(`relay sent no EOSE within ${timeoutMs}ms`));
      }, timeoutMs);
      this.fetches.set(id, { events: [], done: (events) => { clearTimeout(timer); resolve(events); } });
      this.ws!.send(JSON.stringify(['REQ', id, { kinds: [1], '#t': [this.topic, 'reply'], since, limit: 10000 }]));
    });
  }

  /** #4608 — the relay's OK for a publishConfirmed note. True when the frame was one. */
  private handleOkFrame(parsed: unknown[]): boolean {
    const done = parsed[0] === 'OK' && typeof parsed[1] === 'string' ? this.pendingOks.get(parsed[1]) : undefined;
    if (!done) return false;
    this.pendingOks.delete(parsed[1] as string);
    done({ ok: parsed[2] === true, reason: typeof parsed[3] === 'string' ? parsed[3] : '' });
    return true;
  }

  /**
   * #4608 — publish and wait for the relay to say whether it stored the note.
   * publish() fires and forgets, which is how every Jeff note sent through v2
   * was refused on 2026-10-18 with nothing noticing. Never rejects: the answer
   * is ok, or not ok with the relay's own reason (or why there was none).
   */
  publishConfirmed(event: NostrEvent, timeoutMs = 5000): Promise<{ ok: boolean; reason: string }> {
    if (!this.ws || !this.isConnected()) return Promise.resolve({ ok: false, reason: 'relay not connected' });
    return new Promise((resolve) => {
      const timer = setTimeout(() => {
        this.pendingOks.delete(event.id);
        resolve({ ok: false, reason: `relay sent no OK within ${timeoutMs}ms` });
      }, timeoutMs);
      this.pendingOks.set(event.id, (r) => { clearTimeout(timer); resolve(r); });
      this.ws!.send(JSON.stringify(['EVENT', event]));
    });
  }

  /**
   * #4608 — a note signed by this connection's own identity (the bridge), the
   * only key the relay stores from this connection. The room never holds a
   * person's key (#3910); it vouches for a verified sender with the author tag.
   */
  signAsReader(content: string, tags: string[][], createdAt: number): NostrEvent | null {
    if (!this.signer) return null;
    const id = buildEventId(serializeEvent(this.signer.pubkey, createdAt, 1, tags, content));
    return { id, pubkey: this.signer.pubkey, created_at: createdAt, kind: 1, tags, content, sig: this.signer.sign(id) };
  }

  /** #4592 — EVENT/EOSE for a fetchSince subscription. True when the frame was one. */
  private handleFetchFrame(parsed: unknown[]): boolean {
    const f = typeof parsed[1] === 'string' ? this.fetches.get(parsed[1]) : undefined;
    if (!f) return false;
    if (parsed[0] === 'EVENT' && parsed[2]) f.events.push(parsed[2] as NostrEvent);
    if (parsed[0] === 'EOSE') {
      this.fetches.delete(parsed[1] as string);
      if (this.ws) this.ws.send(JSON.stringify(['CLOSE', parsed[1]]));
      f.done(f.events);
    }
    return true;
  }

  private answerChallenge(challenge: string): void {
    this.authState = 'challenged';
    if (!this.signer || !this.ws) {
      this.authState = 'no-signer';
      return;
    }
    const frame = authFrame(challenge, this.authRelayTag(), this.signer, this.nowSec());
    this.authEventId = (JSON.parse(frame)[1] as { id: string }).id;
    this.ws.send(frame);
    this.authState = 'sent';
    // the REQ sent on open was refused before AUTH; ask again, in order after it
    this.subscribe();
  }

  close(): void {
    this.stopped = true;
    if (this.reconnectTimer) { clearTimeout(this.reconnectTimer); this.reconnectTimer = null; }
    if (this.ws) {
      this.ws.close();
      this.ws = null;
      this.connected = false;
    }
  }
}
