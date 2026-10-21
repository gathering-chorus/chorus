// @domain: messages
/**
 * #4629 S1 — pulse reads the team room in shadow, ahead of delivering from it
 * (step B: senders write the room, pulse only delivers). For every #team note
 * it judges whether delivering from Buzz would deliver the same message pulse
 * delivered from its own store, joined exactly by the note's `msg` tag
 * (pulse-<id>, buzz-writer.ts), never by text. It delivers nothing. S3 is gated
 * on a full day of 100% match (Silas, 10-21 06:16).
 */
import WebSocket from 'ws';
import { frameText, signNote, type BuzzNote, type BuzzSigner } from './buzz-writer';

export interface ShadowVerdict {
  msg: string;
  verdict: 'match' | 'to-mismatch' | 'no-row' | 'no-msg-tag' | 'no-to-tag' | 'not-a-delivery';
}

/** The store calls the shadow join needs. */
export interface ShadowStore { getMessageEnds(id: number): { from: string; to: string; type: string } | null }

const tag = (n: BuzzNote, name: string): string | undefined => n.tags.find((t) => t[0] === name)?.[1];

/** Kinds pulse delivers to a terminal; replies, land notices and terminal copies are not deliveries. */
const DELIVERED_KINDS = new Set(['nudge', 'alert']);

export function shadowVerdict(note: BuzzNote, store: ShadowStore): ShadowVerdict {
  const msg = tag(note, 'msg') ?? '';
  if (!DELIVERED_KINDS.has(tag(note, 'kind') ?? '') || tag(note, 'surface') === 'terminal') return { msg, verdict: 'not-a-delivery' };
  const m = /^pulse-(\d+)$/.exec(msg);
  if (!m) return { msg, verdict: 'no-msg-tag' };
  const row = store.getMessageEnds(Number(m[1]));
  if (!row) return { msg, verdict: 'no-row' };
  const to = tag(note, 'to');
  if (!to) return { msg, verdict: 'no-to-tag' };
  return { msg, verdict: to === row.to ? 'match' : 'to-mismatch' };
}

export interface BuzzReaderOptions {
  relayUrl: string;
  hostHeader?: string;
  /** The reader's own identity (principal-pulse), used only to sign in. */
  signer: BuzzSigner;
  store: ShadowStore;
  channel: string;
  onVerdict: (v: ShadowVerdict) => void;
}

/** Subscribes to kind 9 in one channel from now on and judges each note once. */
export class BuzzReader {
  private ws: WebSocket | null = null;
  private closed = false;
  private readonly seen = new Set<string>();
  private readonly since = Math.floor(Date.now() / 1000) - 60;

  constructor(private readonly o: BuzzReaderOptions) {
    this.connect();
  }

  close(): void {
    this.closed = true;
    this.ws?.terminate();
    this.ws = null;
  }

  private connect(): void {
    if (this.closed) return;
    const ws = this.o.hostHeader ? new WebSocket(this.o.relayUrl, { headers: { Host: this.o.hostHeader } }) : new WebSocket(this.o.relayUrl);
    this.ws = ws;
    ws.on('open', () => this.subscribe());
    ws.on('message', (data) => this.onFrame(frameText(data)));
    ws.on('error', () => { /* close follows */ });
    ws.on('close', () => {
      if (this.ws === ws) this.ws = null;
      if (!this.closed) setTimeout(() => this.connect(), 5000).unref();
    });
  }

  private subscribe(): void {
    this.ws?.send(JSON.stringify(['REQ', 'pulse-shadow', { kinds: [9], '#h': [this.o.channel], since: this.since }]));
  }

  private onFrame(raw: string): void {
    let f: unknown[];
    try { f = JSON.parse(raw) as unknown[]; } catch { return; }
    if (f[0] === 'AUTH' && typeof f[1] === 'string') {
      const relay = this.o.hostHeader ? `ws://${this.o.hostHeader}` : this.o.relayUrl;
      const auth = signNote(this.o.signer, Math.floor(Date.now() / 1000), 22242, [['relay', relay], ['challenge', f[1]]], '');
      this.ws?.send(JSON.stringify(['AUTH', auth]));
      // a relay that answered the first REQ only after sign-in gets it again
      this.subscribe();
      return;
    }
    if (f[0] !== 'EVENT' || typeof f[2] !== 'object' || f[2] === null) return;
    const note = f[2] as BuzzNote;
    if (this.seen.has(note.id)) return;
    this.seen.add(note.id);
    this.o.onVerdict(shadowVerdict(note, this.o.store));
  }
}
