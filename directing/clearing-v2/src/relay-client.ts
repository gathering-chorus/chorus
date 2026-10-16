// @domain: messages

import { NostrEvent } from './types';
import { buildEventId } from './identity';

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
  private authState: 'none' | 'challenged' | 'sent' | 'no-signer' = 'none';

  constructor(options: BuzzRelayClientOptions) {
    this.relayUrl = options.relayUrl;
    this.topic = options.topic || 'team';
    this.wsFactory = options.wsFactory;
    this.onEventCallback = options.onEvent;
    this.subId = options.subId || 'team-sub';
    this.signer = options.signer;
    this.hostHeader = options.hostHeader;
    this.nowSec = options.nowSec || (() => Math.floor(Date.now() / 1000));
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
    });

    ws.on('error', () => {
      this.connected = false;
    });
  }

  isConnected(): boolean {
    return this.connected;
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
    try {
      const parsed = JSON.parse(raw);
      if (Array.isArray(parsed) && parsed[0] === 'AUTH' && typeof parsed[1] === 'string') {
        this.authState = 'challenged';
        if (!this.signer || !this.ws) {
          this.authState = 'no-signer';
          return;
        }
        this.ws.send(authFrame(parsed[1], this.relayUrl, this.signer, this.nowSec()));
        this.authState = 'sent';
        // the REQ sent on open was refused before AUTH; ask again, in order after it
        this.subscribe();
        return;
      }
      if (Array.isArray(parsed) && parsed[0] === 'EVENT' && parsed[2]) {
        const event = parsed[2] as NostrEvent;
        if (this.onEventCallback) {
          this.onEventCallback(event);
        }
      }
    } catch {
      // ignore unparseable
    }
  }

  close(): void {
    if (this.ws) {
      this.ws.close();
      this.ws = null;
      this.connected = false;
    }
  }
}
