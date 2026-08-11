/* eslint-disable security/detect-non-literal-fs-filename --
 * The path is the spine log's location, supplied by the server at wiring time
 * from its own config — never from a request. Read-only, and the alternative
 * (a literal) would hardcode one machine's home directory into a handler.
 */
/**
 * #3819 — scan the spine instead of swallowing it.
 *
 * The spine (~/.chorus/chorus.log) is 732MB and NEVER rotates — Jeff's ruling,
 * because rotation truncates reach. Two handlers were reading the whole thing
 * into a JS string per request and then splitting it into several million more
 * strings, on the API's only thread. That is the 3–8 second freeze behind the
 * blocked-event-loop alerts, and it gets worse every day by construction.
 *
 * What replaces it: a bounded read of the TAIL, newest activity first. Card
 * events for one card are sparse and recent — the point of asking for a card's
 * story is what happened to that card, and a card untouched in the last stretch
 * of team activity has no story worth freezing the API for.
 *
 * The cost, stated rather than hidden: this can miss an event older than the
 * window. `truncated` says so, so a caller can tell "nothing happened" from "we
 * stopped looking". That distinction is the whole card — without it, fast and
 * right looks identical to fast and wrong.
 */

import fs from 'fs';

/** How much of the tail to read, in bytes. */
export const DEFAULT_TAIL_BYTES = 32 * 1024 * 1024; // 32MB

export interface SpineScan {
  /** Lines from the window, in file order (oldest first within the window). */
  lines: string[];
  /** True when the file was larger than the window, so older events went unread. */
  truncated: boolean;
  /** Bytes actually read. */
  bytesRead: number;
}

/**
 * Read the last `maxBytes` of a file as lines.
 *
 * A missing or unreadable file returns an empty scan rather than throwing: the
 * story surfaces then degrade to "no spine events", which is exactly what they
 * already did when the read failed.
 *
 * The first line of the window is DISCARDED when the file was truncated. A
 * window starting mid-file almost certainly starts mid-line, and half a JSON
 * object parses as nothing — it would be dropped anyway, silently. Dropping it
 * deliberately is the same act done knowingly.
 */
export function scanTail(file: string, maxBytes: number = DEFAULT_TAIL_BYTES): SpineScan {
  let fd: number;
  try {
    fd = fs.openSync(file, 'r');
  } catch {
    return { lines: [], truncated: false, bytesRead: 0 };
  }
  try {
    const size = fs.fstatSync(fd).size;
    const bytesRead = Math.min(size, maxBytes);
    const start = size - bytesRead;
    const buf = Buffer.alloc(bytesRead);
    fs.readSync(fd, buf, 0, bytesRead, start);
    const lines = buf.toString('utf8').split('\n');
    const truncated = start > 0;
    if (truncated) lines.shift();
    return { lines: lines.filter((l) => l.length > 0), truncated, bytesRead };
  } catch {
    return { lines: [], truncated: false, bytesRead: 0 };
  } finally {
    fs.closeSync(fd);
  }
}

/**
 * Parse the lines matching a predicate.
 *
 * `match` runs on the RAW line before JSON.parse — a substring test is orders
 * of magnitude cheaper than parsing, and on this file the overwhelming majority
 * of lines are not the ones being asked for.
 */
export function parseMatching<T>(
  lines: string[],
  match: (line: string) => boolean,
  take: (parsed: Record<string, unknown>) => T | null,
): T[] {
  const out: T[] = [];
  for (const line of lines) {
    if (!match(line)) continue;
    let parsed: Record<string, unknown>;
    try {
      parsed = JSON.parse(line) as Record<string, unknown>;
    } catch {
      continue; // a malformed line is not an error, it is a line
    }
    const value = take(parsed);
    if (value !== null) out.push(value);
  }
  return out;
}
