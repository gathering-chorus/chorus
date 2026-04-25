/* eslint-disable security/detect-non-literal-fs-filename --
 * Server-controlled CHORUS_LOG path under CHORUS_ROOT.
 */
import * as fs from 'fs';
import * as path from 'path';

const LOG_FILE = path.resolve(__dirname, '../../logs/chorus.log');

export interface SubscribeOptions {
  logFile?: string;
  pollInterval?: number;
}

type EventFilter = string | RegExp | ((event: string) => boolean);
type EventCallback = (event: Record<string, string>) => void;

/**
 * Subscribe to new spine events matching a filter.
 * Returns an unsubscribe function.
 */
export function subscribe(
  filter: EventFilter,
  callback: EventCallback,
  options: SubscribeOptions = {},
): () => void {
  const target = options.logFile ?? LOG_FILE;
  const interval = options.pollInterval ?? 1000;

  let lastSize = 0;
  try {
    const stat = fs.statSync(target);
    lastSize = stat.size;
  } catch { /* file may not exist yet */ }

  const matchFn: (event: string) => boolean =
    typeof filter === 'string'
      ? (e) => e === filter || e.includes(filter)
      : filter instanceof RegExp
        ? (e) => filter.test(e)
        : filter;

  const timer = setInterval(() => {
    try {
      const stat = fs.statSync(target);
      if (stat.size <= lastSize) return;

      const fd = fs.openSync(target, 'r');
      const buf = Buffer.alloc(stat.size - lastSize);
      fs.readSync(fd, buf, 0, buf.length, lastSize);
      fs.closeSync(fd);
      lastSize = stat.size;

      const lines = buf.toString('utf-8').split('\n').filter(Boolean);
      for (const line of lines) {
        try {
          const parsed = JSON.parse(line);
          if (parsed.event && matchFn(parsed.event)) {
            callback(parsed);
          }
        } catch { /* skip malformed lines */ }
      }
    } catch { /* best effort */ }
  }, interval);

  return () => clearInterval(timer);
}
