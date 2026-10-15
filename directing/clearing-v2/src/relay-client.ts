// @domain: messages

import { NostrEvent } from './types';

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

  constructor(options: BuzzRelayClientOptions) {
    this.relayUrl = options.relayUrl;
    this.topic = options.topic || 'team';
    this.wsFactory = options.wsFactory;
    this.onEventCallback = options.onEvent;
    this.subId = options.subId || 'team-sub';
  }

  connect(): void {
    if (this.wsFactory) {
      this.ws = this.wsFactory();
    } else {
      // Lazy load 'ws' when running in Node.js
      const WebSocket = require('ws');
      this.ws = new WebSocket(this.relayUrl);
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
      '#t': [this.topic],
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
