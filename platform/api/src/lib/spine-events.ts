/* eslint-disable security/detect-non-literal-fs-filename -- the spine path is fixed by the caller (LOG_PATHS.chorus), never request input (#4431) */
/**
 * #4431 — the one reader of the spine. Jeff, 2026-10-05: "to me spine is an
 * events domain endpoint" and "n consumers all interacting with it
 * differently".
 *
 * Measured that day: the in-process freeze detector put 20 of 41 chorus-api
 * freezes on GET /context/roles, which read the last 4 MB of chorus.log three
 * times per call (once per role), synchronously, about every 2 s. Every other
 * reader carried its own copy of the same read.
 *
 * Here the tail is read ONCE per refresh window, off the event loop
 * (fs.promises), parsed once, and every consumer filters the same parsed list.
 * Concurrent callers inside a window share one read; a caller after the window
 * triggers a fresh one. The file is never opened by a consumer.
 */
import { promises as fsp } from 'fs';

/** One spine line, as the events domain serves it (SpineEvent: eventKind, emittedBy, timestamp). */
export interface SpineEventRow {
  timestamp: string;
  /** epoch ms of `timestamp`, for filtering. */
  ts: number;
  event: string;
  role?: string;
  card_id?: string | number;
  detail?: string;
  payload?: string;
}

export interface SpineEventQuery {
  role?: string;
  /** Exact event names; any of them. */
  types?: string[];
  sinceMs?: number;
  /** Most recent N after filtering. */
  limit?: number;
}

export interface SpineEventsReaderOptions {
  path: string;
  tailBytes?: number;
  /** A read is reused for this long. */
  refreshMs?: number;
  now?: () => number;
}

export function parseSpineLine(line: string): SpineEventRow | null {
  if (!line || line[0] !== '{') return null;
  let p: Record<string, unknown>;
  try { p = JSON.parse(line) as Record<string, unknown>; } catch { return null; }
  if (typeof p.event !== 'string' || typeof p.timestamp !== 'string') return null;
  const ts = Date.parse(p.timestamp);
  if (!Number.isFinite(ts)) return null;
  return {
    timestamp: p.timestamp,
    ts,
    event: p.event,
    role: typeof p.role === 'string' ? p.role : undefined,
    card_id: typeof p.card_id === 'string' || typeof p.card_id === 'number' ? p.card_id : undefined,
    detail: typeof p.detail === 'string' ? p.detail : undefined,
    payload: typeof p.payload === 'string' ? p.payload : undefined,
  };
}

export function filterSpineEvents(rows: SpineEventRow[], q: SpineEventQuery): SpineEventRow[] {
  const types = q.types && q.types.length > 0 ? new Set(q.types) : null;
  const out = rows.filter((r) =>
    (q.role === undefined || r.role === q.role)
    && (types === null || types.has(r.event))
    && (q.sinceMs === undefined || r.ts >= q.sinceMs));
  return q.limit !== undefined && q.limit >= 0 ? out.slice(-q.limit) : out;
}

export class SpineEventsReader {
  private readonly path: string;
  private readonly tailBytes: number;
  private readonly refreshMs: number;
  private readonly now: () => number;
  private rows: SpineEventRow[] = [];
  private readAt = -Infinity;
  private inflight: Promise<SpineEventRow[]> | null = null;
  /** How many times the file has been read; tests use it as the negative proof. */
  reads = 0;

  constructor(o: SpineEventsReaderOptions) {
    this.path = o.path;
    this.tailBytes = o.tailBytes ?? 4 * 1024 * 1024;
    this.refreshMs = o.refreshMs ?? 2000;
    this.now = o.now ?? Date.now;
  }

  /** The parsed tail, oldest first. Shared by every caller inside one refresh window.
   *  A spine that cannot be read rejects; the caller says so, it never reads as empty. */
  async recent(): Promise<SpineEventRow[]> {
    if (this.now() - this.readAt < this.refreshMs) return this.rows;
    if (this.inflight) return this.inflight;
    this.inflight = this.readTail()
      .then((rows) => { this.rows = rows; this.readAt = this.now(); return rows; })
      .finally(() => { this.inflight = null; });
    return this.inflight;
  }

  async query(q: SpineEventQuery): Promise<SpineEventRow[]> {
    return filterSpineEvents(await this.recent(), q);
  }

  private async readTail(): Promise<SpineEventRow[]> {
    this.reads += 1;
    let fh: import('fs').promises.FileHandle | null = null;
    try {
      fh = await fsp.open(this.path, 'r');
      const { size } = await fh.stat();
      const len = Math.min(size, this.tailBytes);
      const buf = Buffer.alloc(len);
      await fh.read(buf, 0, len, size - len);
      const text = buf.toString('utf-8');
      // A tail cut mid-line leaves a partial first line; parseSpineLine drops it.
      const out: SpineEventRow[] = [];
      for (const line of text.split('\n')) {
        const r = parseSpineLine(line);
        if (r) out.push(r);
      }
      return out;
    } finally {
      if (fh) await fh.close();
    }
  }
}
