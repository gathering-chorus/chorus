// @domain: events
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

/**
 * #4438 — the event types a producer domain owns and/or that are about a model
 * class, read from the registry (spine-events.json, generated from the
 * EventType rows). undefined = no producer/about asked. An empty array means
 * nothing matches, and the caller must return no events, never all of them.
 */
export function typesFor(
  registry: Record<string, { producer?: string; about?: string }>,
  q: { producer?: string; about?: string },
): string[] | undefined {
  if (q.producer === undefined && q.about === undefined) return undefined;
  return Object.entries(registry)
    .filter(([, e]) => (q.producer === undefined || e.producer === q.producer)
      && (q.about === undefined || e.about === q.about))
    .map(([name]) => name);
}

const param = (v: unknown): string | undefined => (typeof v === 'string' && v ? v : undefined);

/**
 * #4438 — the GET /api/chorus/spine-events query: role, type (comma list),
 * since, limit, producer, about. producer/about narrow the types from the
 * registry. null = the filters match no type, so the answer is no events.
 */
export function parseSpineEventsQuery(
  query: Record<string, unknown>,
  registry: Record<string, { producer?: string; about?: string }>,
): SpineEventQuery | null {
  let types = param(query.type)?.split(',');
  const owned = typesFor(registry, { producer: param(query.producer), about: param(query.about) });
  if (owned !== undefined) types = types ? types.filter((t) => owned.includes(t)) : owned;
  if (types !== undefined && types.length === 0) return null;
  const since = Date.parse(param(query.since) ?? '');
  const limit = Math.min(2000, Math.max(1, parseInt(param(query.limit) ?? '500', 10) || 500));
  return { role: param(query.role), types, sinceMs: Number.isFinite(since) ? since : undefined, limit };
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
  /** Bytes read from the file, all reads together; a refresh reads only what was added. */
  bytesRead = 0;
  /** File position just past the last whole line taken in; -1 before the first read. */
  private offset = -1;
  /** Byte length of each row's line, in step with rows, so the window stays tailBytes. */
  private lens: number[] = [];

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

  /**
   * Reads only the bytes appended since the last read (#4431 reopen: re-parsing the
   * whole 4 MB tail every refresh blocked chorus-api's event loop for 3–4.5 s). The
   * first read, a file that shrank, or a gap wider than the window read the tail
   * afresh. Only whole lines are taken: a line still being written waits for its
   * newline, so it is never parsed in halves.
   */
  private async readTail(): Promise<SpineEventRow[]> {
    this.reads += 1;
    let fh: import('fs').promises.FileHandle | null = null;
    try {
      fh = await fsp.open(this.path, 'r');
      const { size } = await fh.stat();
      const fresh = this.offset < 0 || size < this.offset || size - this.offset > this.tailBytes;
      const start = fresh ? Math.max(0, size - this.tailBytes) : this.offset;
      const buf = Buffer.alloc(size - start);
      if (buf.length > 0) await fh.read(buf, 0, buf.length, start);
      this.bytesRead += buf.length;
      const end = buf.lastIndexOf(0x0a);
      let rows = fresh ? [] : this.rows;
      let lens = fresh ? [] : this.lens;
      const added: SpineEventRow[] = [];
      const addedLens: number[] = [];
      if (end >= 0) {
        // A fresh tail cut mid-line leaves a partial first line; parseSpineLine drops it.
        for (const line of buf.subarray(0, end).toString('utf-8').split('\n')) {
          const r = parseSpineLine(line);
          if (r) { added.push(r); addedLens.push(Buffer.byteLength(line) + 1); }
        }
      }
      this.offset = end >= 0 ? start + end + 1 : (fresh ? start : this.offset);
      rows = rows.concat(added);
      lens = lens.concat(addedLens);
      let bytes = lens.reduce((a, b) => a + b, 0);
      let drop = 0;
      while (bytes > this.tailBytes && drop < lens.length) bytes -= lens[drop++];
      this.rows = drop > 0 ? rows.slice(drop) : rows;
      this.lens = drop > 0 ? lens.slice(drop) : lens;
      return this.rows;
    } finally {
      if (fh) await fh.close();
    }
  }
}
