// #4508 — what chorus-api was doing during a stall.
//
// 117 stall alerts on 2026-10-12, 86 of them naming no cause: the external
// probe announced first, from outside, while the loop was still stuck, so it
// could only say "probe-timeout". This ledger keeps the last requests (start,
// end, path, bytes) in memory so that, once the loop answers again, either
// detector can name the requests whose time overlapped the stall. Static file
// serves are included: they never reached the access log before (#3058's line
// was mounted after express.static).

export interface LedgerEntry {
  id: number;
  method: string;
  path: string;
  start: number;
  end: number | null;
  status: number | null;
  bytes: number | null;
}

export class RequestLedger {
  private entries: LedgerEntry[] = [];
  private nextId = 1;

  constructor(private readonly capacity = 1000) {}

  start(method: string, path: string, now: number): number {
    const id = this.nextId++;
    this.entries.push({ id, method, path, start: now, end: null, status: null, bytes: null });
    if (this.entries.length > this.capacity) {
      // drop the oldest finished entry; an unfinished one stays until it ends
      const i = this.entries.findIndex((e) => e.end !== null);
      this.entries.splice(i >= 0 ? i : 0, 1);
    }
    return id;
  }

  finish(id: number, status: number, bytes: number | null, now: number): void {
    const e = this.entries.find((x) => x.id === id);
    if (!e) return;
    e.end = now;
    e.status = status;
    e.bytes = bytes;
  }

  /** Requests whose [start, end] overlaps [from, to]; an unfinished one is open-ended. */
  overlapping(from: number, to: number): LedgerEntry[] {
    return this.entries.filter((e) => e.start <= to && (e.end === null || e.end >= from));
  }
}

/** One plain sentence naming what was running, longest first. A stall with
 *  nothing in flight says so: then the block was outside any request (a timer,
 *  a scheduled job, garbage collection), never a blank. */
export function describeOverlap(entries: LedgerEntry[], now: number, max = 3): string {
  if (entries.length === 0) {
    return 'No request was in flight during the stall, so it ran outside any request (a timer or scheduled job).';
  }
  const dur = (e: LedgerEntry) => (e.end ?? now) - e.start;
  const named = [...entries].sort((a, b) => dur(b) - dur(a)).slice(0, max).map((e) => {
    const size = e.bytes !== null ? `, ${e.bytes} bytes` : '';
    const state = e.end === null ? ', still running' : '';
    return `${e.method} ${e.path} (${(dur(e) / 1000).toFixed(1)}s${size}${state})`;
  });
  const more = entries.length > max ? ` and ${entries.length - max} more` : '';
  return `In flight during the stall: ${named.join('; ')}${more}.`;
}
