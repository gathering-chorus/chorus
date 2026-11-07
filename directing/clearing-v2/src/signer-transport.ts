// @domain: messages
// #4652 reopen 2 — the room's line to Jeff's signing service: one relay connection,
// AUTHed with the room's own client key (the relay serves and stores only for the AUTH
// identity, #4608), carrying the kind 24133 requests and answers.
import { schnorr } from '@noble/curves/secp256k1';
import { bytesToHex, hexToBytes, randomBytes } from '@noble/hashes/utils';
import { buildEventId, serializeEvent } from './identity';
import { authRelayFor, WebSocketLike } from './relay-client';
import { SignerTransport } from './remote-signer';
import { NostrEvent } from './types';

export interface RelaySignerTransportOptions {
  relayUrl: string;
  /** The room's own client key (hex) — never Jeff's. */
  clientSecret: string;
  hostHeader?: string;
  wsFactory?: () => WebSocketLike;
}

export class RelaySignerTransport implements SignerTransport {
  private ws: WebSocketLike | null = null;
  private ready: Promise<void> | null = null;
  private readonly pending = new Map<string, (r: { ok: boolean; reason: string }) => void>();
  private readonly subs = new Map<string, (e: NostrEvent) => void>();
  private readonly pubkey: string;
  private authId = '';

  constructor(private readonly o: RelaySignerTransportOptions) {
    this.pubkey = bytesToHex(schnorr.getPublicKey(hexToBytes(o.clientSecret)));
  }

  async publish(event: NostrEvent): Promise<{ ok: boolean; reason: string }> {
    try { await this.connect(); } catch (e) { return { ok: false, reason: (e as Error).message }; }
    return new Promise((resolve) => {
      this.pending.set(event.id, resolve);
      this.ws!.send(JSON.stringify(['EVENT', event]));
    });
  }

  subscribe(filter: Record<string, unknown>, onEvent: (e: NostrEvent) => void): () => void {
    const id = `signer-${bytesToHex(randomBytes(6))}`;
    this.subs.set(id, onEvent);
    void this.connect().then(() => this.ws!.send(JSON.stringify(['REQ', id, { ...filter, since: Math.floor(Date.now() / 1000) - 5 }]))).catch(() => undefined);
    return () => { this.subs.delete(id); try { this.ws?.send(JSON.stringify(['CLOSE', id])); } catch { /* closed */ } };
  }

  close(): void { try { this.ws?.close(); } catch { /* closed */ } this.ws = null; this.ready = null; }

  private authRelayUrl(): string {
    return authRelayFor(this.o.relayUrl, this.o.hostHeader);
  }

  private connect(): Promise<void> {
    if (this.ready) return this.ready;
    this.ready = new Promise<void>((resolve, reject) => {
      const ws = this.open();
      this.ws = ws;
      const timer = setTimeout(() => { this.ready = null; reject(new Error('relay sign-in for the signing service timed out')); }, 8000);
      ws.on('error', () => { clearTimeout(timer); this.ready = null; reject(new Error('relay connection failed')); });
      ws.on('close', () => { this.ws = null; this.ready = null; });
      ws.on('message', (raw: unknown) => this.onMessage(ws, raw, () => { clearTimeout(timer); resolve(); }, (e) => { clearTimeout(timer); this.ready = null; reject(e); }));
    });
    return this.ready;
  }

  private onMessage(ws: WebSocketLike, raw: unknown, authed: () => void, refused: (e: Error) => void): void {
    let m: unknown[];
    try { m = JSON.parse(String(raw)); } catch { return; }
    const [type, a, b, c] = m;
    if (type === 'AUTH' && typeof a === 'string') this.answerChallenge(ws, a);
    else if (type === 'OK' && a === this.authId) { if (b === true) authed(); else refused(new Error(`relay refused the room's sign-in: ${reasonOf(c)}`)); }
    else if (type === 'OK' && typeof a === 'string') this.settle(a, b === true, reasonOf(c));
    else if (type === 'EVENT' && typeof a === 'string' && b) this.subs.get(a)?.(b as NostrEvent);
  }

  private answerChallenge(ws: WebSocketLike, challenge: string): void {
    const auth = this.sign(22242, [['relay', this.authRelayUrl()], ['challenge', challenge]], '');
    this.authId = auth.id;
    ws.send(JSON.stringify(['AUTH', auth]));
  }

  private settle(id: string, ok: boolean, reason: string): void {
    const done = this.pending.get(id);
    if (!done) return;
    this.pending.delete(id);
    done({ ok, reason });
  }

  private open(): WebSocketLike {
    if (this.o.wsFactory) return this.o.wsFactory();
    const WebSocket = require('ws');
    return this.o.hostHeader ? new WebSocket(this.o.relayUrl, { headers: { Host: this.o.hostHeader } }) : new WebSocket(this.o.relayUrl);
  }

  private sign(kind: number, tags: string[][], content: string): NostrEvent {
    const created_at = Math.floor(Date.now() / 1000);
    const id = buildEventId(serializeEvent(this.pubkey, created_at, kind, tags, content));
    return { id, pubkey: this.pubkey, created_at, kind, tags, content, sig: bytesToHex(schnorr.sign(hexToBytes(id), hexToBytes(this.o.clientSecret))) };
  }
}

const reasonOf = (v: unknown): string => (typeof v === 'string' ? v : '');
