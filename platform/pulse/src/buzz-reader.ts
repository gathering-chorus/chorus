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
import { body, frameText, signNote, type BuzzNote, type BuzzSigner } from './buzz-writer';

export interface ShadowVerdict {
  msg: string;
  /** missed: pulse recorded the message but no room note named it within the grace window (Silas 06:22). */
  verdict: 'match' | 'to-mismatch' | 'no-row' | 'no-msg-tag' | 'no-to-tag' | 'not-a-delivery' | 'missed'
    /** #4629 AC4 — signed by a key that is not the author's (refused), or no registry read yet (fail closed). */
    | 'wrong-key' | 'unverified'
    /** #4629 S3 — the note's words differ from the row's: refused, so Buzz is the message and not only a gate (Silas 06:42) */
    | 'content-mismatch';
  /** the author tag, on a wrong-key verdict, so the sender can be told */
  author?: string;
}

/** The store calls the shadow join needs. */
export interface ShadowStore {
  getMessageEnds(id: number): { from: string; to: string; type: string; content: string } | null;
  getMessageEndsByTrace(trace: string): { id: number; from: string; to: string; type: string; content: string } | null;
  nudgeIdsBetween(from: string, to: string): number[];
}

/** A time as sqlite's datetime('now') writes it, UTC. */
export const sqliteUtc = (ms: number): string => new Date(ms).toISOString().slice(0, 19).replace('T', ' ');

const tag = (n: BuzzNote, name: string): string | undefined => n.tags.find((t) => t[0] === name)?.[1];

/** Kinds pulse delivers to a terminal; replies, land notices and terminal copies are not deliveries. */
const DELIVERED_KINDS = new Set(['nudge', 'alert']);

/** The store row a note names: pulse-<id> on bridge copies, trace-<uuid> on sender-signed notes (#4629 S2). */
function rowFor(msg: string, store: ShadowStore): { id: number; to: string; content: string } | null | undefined {
  const byId = /^pulse-(\d+)$/.exec(msg);
  if (byId) { const r = store.getMessageEnds(Number(byId[1])); return r ? { id: Number(byId[1]), to: r.to, content: r.content } : null; }
  const byTrace = /^trace-([0-9a-f-]{36})$/.exec(msg);
  if (byTrace) { const r = store.getMessageEndsByTrace(byTrace[1]); return r ? { id: r.id, to: r.to, content: r.content } : null; }
  return undefined;
}

/** The words a room note carries, without the room's "[nudge from x → y]" or "[alert → y]" lead. */
export function noteWords(content: string): string {
  return content.replace(/^\[(nudge from [^\]→]+→ [^\]]+|alert → [^\]]+)\]\s*/, '');
}

export function shadowVerdict(note: BuzzNote, store: ShadowStore): ShadowVerdict & { id?: number } {
  const msg = tag(note, 'msg') ?? '';
  if (!DELIVERED_KINDS.has(tag(note, 'kind') ?? '') || tag(note, 'surface') === 'terminal') return { msg, verdict: 'not-a-delivery' };
  const row = rowFor(msg, store);
  if (row === undefined) return { msg, verdict: 'no-msg-tag' };
  if (row === null) return { msg, verdict: 'no-row' };
  const to = tag(note, 'to');
  if (!to) return { msg, verdict: 'no-to-tag', id: row.id };
  if (to !== row.to) return { msg, verdict: 'to-mismatch', id: row.id };
  return { msg, verdict: noteWords(note.content) === body(row.content) ? 'match' : 'content-mismatch', id: row.id };
}

export interface BuzzReaderOptions {
  relayUrl: string;
  hostHeader?: string;
  /** The reader's own identity (principal-pulse), used only to sign in. */
  signer: BuzzSigner;
  store: ShadowStore;
  channel: string;
  onVerdict: (v: ShadowVerdict) => void;
  /** #4629 S3 — a delivery note joined its row with the right key: the store id to deliver. */
  onMatch?: (id: number) => void;
  /** #4629 AC4 — whose key signed a note; absent = no key check (S1 behaviour). */
  keys?: { check(pubkey: string, author: string | undefined, msg?: string): Promise<'ok' | 'wrong-key' | 'unverified'> };
  /** How long a recorded message may go without a room note before it counts as missed (default 10 min). */
  graceMs?: number;
  /** How often the missed sweep runs (default 60 s; 0 = only when sweep() is called). */
  sweepEveryMs?: number;
  now?: () => number;
}

/**
 * Subscribes to kind 9 in one channel from now on and judges each note once.
 * A reconnect resumes from the newest note seen (less a minute of overlap the
 * seen-id set absorbs), and a sweep reports every message pulse recorded since
 * start that no note named within the grace window (the reverse direction).
 */
export class BuzzReader {
  private ws: WebSocket | null = null;
  private closed = false;
  private readonly seen = new Set<string>();
  private readonly seenMsg = new Set<number>();
  private readonly reported = new Set<number>();
  private readonly now: () => number;
  private readonly startedAt: number;
  private watermark: number;
  private timer: ReturnType<typeof setInterval> | null = null;

  constructor(private readonly o: BuzzReaderOptions) {
    this.now = o.now ?? Date.now;
    this.startedAt = this.now();
    this.watermark = Math.floor(this.startedAt / 1000) - 60;
    const every = o.sweepEveryMs ?? 60_000;
    if (every > 0) { this.timer = setInterval(() => this.sweep(), every); this.timer.unref(); }
    this.connect();
  }

  /** The subscription's since: the newest note seen, less a minute. */
  since(): number { return this.watermark; }

  close(): void {
    this.closed = true;
    if (this.timer) clearInterval(this.timer);
    this.ws?.terminate();
    this.ws = null;
  }

  /** Report each message recorded since start, older than the grace window, that no room note named. */
  sweep(): void {
    const cutoff = this.now() - (this.o.graceMs ?? 600_000);
    if (cutoff <= this.startedAt) return;
    for (const id of this.o.store.nudgeIdsBetween(sqliteUtc(this.startedAt), sqliteUtc(cutoff))) {
      if (this.seenMsg.has(id) || this.reported.has(id)) continue;
      this.reported.add(id);
      this.o.onVerdict({ msg: `pulse-${id}`, verdict: 'missed' });
    }
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
    this.ws?.send(JSON.stringify(['REQ', 'pulse-shadow', { kinds: [9], '#h': [this.o.channel], since: this.watermark }]));
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
    void this.onNote(f[2] as BuzzNote);
  }

  private async onNote(note: BuzzNote): Promise<void> {
    if (this.seen.has(note.id)) return;
    this.seen.add(note.id);
    if (typeof note.created_at === 'number') this.watermark = Math.max(this.watermark, note.created_at - 60);
    const { id, ...v } = shadowVerdict(note, this.o.store);
    if (v.verdict !== 'not-a-delivery' && this.o.keys) {
      const author = tag(note, 'author');
      const k = await this.o.keys.check(note.pubkey, author, v.msg);
      if (k !== 'ok') { this.o.onVerdict({ msg: v.msg, verdict: k, ...(k === 'wrong-key' && author ? { author } : {}) }); return; }
    }
    if (id !== undefined) this.seenMsg.add(id);
    this.o.onVerdict(v);
    if (id !== undefined && v.verdict === 'match') this.o.onMatch?.(id);
  }
}
