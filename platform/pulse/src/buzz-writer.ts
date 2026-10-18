/**
 * #4609 step A — pulse writes every nudge and alert to Buzz itself.
 *
 * Until now Buzz got nudges only as copies from Clearing v1's spine tailer
 * (#4592), which stopped silently whenever v1 did. Pulse already holds every
 * message, so it is the one writer: a bridge-signed note per message, with the
 * sender in an author tag, the kind (nudge or alert) and pulse's own message id.
 *
 * Delivery never waits on the relay (Wren 14:29: a 5 s relay wait on every
 * nudge under load would be a new stall). post() sends and returns; the
 * relay's answer is checked afterwards. A refusal, silence, or no connection
 * is a miss: reported every time, alerted at most once per window, and the
 * message is delivered regardless (Jeff 14:36: deliver anyway on relay down).
 *
 * The relay stores only notes signed by the connection's own AUTH identity
 * (measured 10-18), so this connection AUTHs as the bridge and signs as the
 * bridge. Jeff input is not written here: Clearing v2 writes it once at the
 * source (#4608).
 */
import fs from 'fs';
import path from 'path';
import WebSocket from 'ws';
import { schnorr } from '@noble/curves/secp256k1';
import { sha256 } from '@noble/hashes/sha256';
import { bytesToHex, hexToBytes, utf8ToBytes } from '@noble/hashes/utils';

export interface BuzzSigner { pubkey: string; sign(idHex: string): string }

export interface BuzzNote {
  id: string; pubkey: string; created_at: number; kind: number;
  tags: string[][]; content: string; sig: string;
}

/** One message pulse carries, as the room should show it. */
export interface BuzzMessage { id: number; from: string; to: string; content: string }

export interface BuzzWriterOptions {
  relayUrl: string;
  signer: BuzzSigner;
  /** The relay's own address for the AUTH tag when dialled through a tunnel. */
  hostHeader?: string;
  topic?: string;
  okTimeoutMs?: number;
  alertEveryMs?: number;
  /** Called for every miss; `alert` is true at most once per alert window. */
  onMiss: (note: BuzzNote, reason: string, alert: boolean) => void;
}

/** Pulse stores a nudge with its delivery header; the room carries the words. */
function body(content: string): string {
  return content.replace(/^\[nudge from [^\]|]+\|[^\]]*\]\s*/, '');
}

/** The text and kind the room shows for a message (same as clearing/src/tailer.ts). */
export function roomText(m: BuzzMessage): { text: string; kind: 'nudge' | 'alert' } {
  const words = body(m.content);
  if (m.from === 'system') return { text: `[alert → ${m.to}] ${words}`, kind: 'alert' };
  if (m.to === 'jeff') return { text: words, kind: 'nudge' };
  return { text: `[nudge from ${m.from} → ${m.to}] ${words}`, kind: 'nudge' };
}

/** A relay frame as text, whichever buffer shape ws hands over. */
function frameText(data: WebSocket.RawData): string {
  if (Array.isArray(data)) return Buffer.concat(data).toString('utf8');
  return Buffer.from(data as ArrayBuffer).toString('utf8');
}

function signNote(signer: BuzzSigner, createdAt: number, kind: number, tags: string[][], content: string): BuzzNote {
  const id = bytesToHex(sha256(utf8ToBytes(JSON.stringify([0, signer.pubkey, createdAt, kind, tags, content]))));
  return { id, pubkey: signer.pubkey, created_at: createdAt, kind, tags, content, sig: signer.sign(id) };
}

export class BuzzWriter {
  private ws: WebSocket | null = null;
  private open = false;
  private closed = false;
  private pending = new Map<string, { note: BuzzNote; timer: ReturnType<typeof setTimeout> }>();
  private lastAlert = 0;
  private opened: Promise<void>;
  private markOpen: () => void = () => {};

  constructor(private readonly o: BuzzWriterOptions) {
    this.opened = new Promise((r) => { this.markOpen = r; });
    this.connect();
  }

  /** Resolves once the socket is open, or after 2 s either way. */
  ready(): Promise<void> {
    return Promise.race([this.opened, new Promise<void>((r) => setTimeout(r, 2000))]);
  }

  /** Post one message. Never throws and never blocks the caller. */
  post(m: BuzzMessage): void {
    const { text, kind } = roomText(m);
    const tags = [['t', this.o.topic ?? 'team'], ['author', m.from], ['kind', kind], ['msg', `pulse-${m.id}`]];
    const note = signNote(this.o.signer, Math.floor(Date.now() / 1000), 1, tags, text);
    if (!this.ws || !this.open) { this.miss(note, 'relay not connected'); return; }
    const timer = setTimeout(() => {
      this.pending.delete(note.id);
      this.miss(note, `relay sent no OK within ${this.o.okTimeoutMs ?? 5000}ms`);
    }, this.o.okTimeoutMs ?? 5000);
    this.pending.set(note.id, { note, timer });
    this.ws.send(JSON.stringify(['EVENT', note]));
  }

  close(): void {
    this.closed = true;
    for (const p of this.pending.values()) clearTimeout(p.timer);
    this.pending.clear();
    this.ws?.terminate();
    this.ws = null;
  }

  private miss(note: BuzzNote, reason: string): void {
    const now = Date.now();
    const alert = now - this.lastAlert >= (this.o.alertEveryMs ?? 600_000);
    if (alert) this.lastAlert = now;
    this.o.onMiss(note, reason, alert);
  }

  private connect(): void {
    if (this.closed) return;
    const ws = this.o.hostHeader ? new WebSocket(this.o.relayUrl, { headers: { Host: this.o.hostHeader } }) : new WebSocket(this.o.relayUrl);
    this.ws = ws;
    ws.on('open', () => { this.open = true; this.markOpen(); });
    ws.on('message', (data) => this.onFrame(frameText(data)));
    ws.on('error', () => { /* close follows */ });
    ws.on('close', () => {
      this.open = false;
      if (this.ws === ws) this.ws = null;
      if (!this.closed) setTimeout(() => this.connect(), 5000).unref();
    });
  }

  private onFrame(raw: string): void {
    let f: unknown[];
    try { f = JSON.parse(raw) as unknown[]; } catch { return; }
    if (f[0] === 'AUTH' && typeof f[1] === 'string') {
      // NIP-42: answer the challenge as the bridge, the identity this connection writes as
      const relay = this.o.hostHeader ? `ws://${this.o.hostHeader}` : this.o.relayUrl;
      const auth = signNote(this.o.signer, Math.floor(Date.now() / 1000), 22242, [['relay', relay], ['challenge', f[1]]], '');
      this.ws?.send(JSON.stringify(['AUTH', auth]));
      return;
    }
    if (f[0] !== 'OK' || typeof f[1] !== 'string') return;
    const p = this.pending.get(f[1]);
    if (!p) return;
    clearTimeout(p.timer);
    this.pending.delete(f[1]);
    if (f[2] !== true) this.miss(p.note, typeof f[3] === 'string' && f[3] ? f[3] : 'relay refused');
  }
}

/**
 * The bridge identity's signer, from the key file chorus-nostr-key minted
 * (~/.chorus/identity/bridge/nostr.json), the same key Clearing v1 and v2 write
 * the room with. The key stays in that file; callers get a signer, never the key.
 */
export function bridgeSignerFromHome(home: string): { signer?: BuzzSigner; reason?: string } {
  const file = path.join(home, '.chorus', 'identity', 'bridge', 'nostr.json');
  let parsed: { seckey?: unknown; pubkey?: unknown };
  try {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- fixed filename under the caller's home
    parsed = JSON.parse(fs.readFileSync(file, 'utf8')) as { seckey?: unknown; pubkey?: unknown };
  } catch {
    return { reason: `no bridge key at ${file}` };
  }
  if (typeof parsed.seckey !== 'string' || !/^[0-9a-f]{64}$/i.test(parsed.seckey)) return { reason: `${file} has no 32-byte hex seckey` };
  const sk = hexToBytes(parsed.seckey);
  const pubkey = bytesToHex(schnorr.getPublicKey(sk));
  if (typeof parsed.pubkey === 'string' && parsed.pubkey !== pubkey) return { reason: `${file} pubkey does not match its seckey` };
  return { signer: { pubkey, sign: (idHex: string) => bytesToHex(schnorr.sign(hexToBytes(idHex), sk)) } };
}
