/**
 * Shared log-reader for chorus-api summary handlers (#2126).
 *
 * Four call sites in server.ts re-implemented the same
 * `() => fs.existsSync(p) ? fs.readFileSync(p, 'utf-8') : null` pattern
 * (chorus-card-story, chorus-domain-story, chorus-domain readDomainHtml,
 * chorus-hooks-metrics). Each new handler drifted the convention.
 * Extract earned at the second duplicate.
 */
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';

// #3197 — root resolution lives in one place (chorus-paths). Re-exported here
// so existing importers of CHORUS_ROOT from log-reader keep their surface.
import { CHORUS_ROOT } from './chorus-paths';
export { CHORUS_ROOT };

export const LOG_PATHS = {
  chorus: path.join(os.homedir(), '.chorus/chorus.log'),
  hooks: path.join(os.homedir(), 'Library/Logs/Gathering/hooks.log'),
  permissions: path.join(CHORUS_ROOT, 'platform/logs/permissions.log'),
  errors: path.join(CHORUS_ROOT, 'platform/logs/errors.log'),
  handoffs: path.join(CHORUS_ROOT, 'platform/logs/handoffs.log'),
} as const;

export type LogName = keyof typeof LOG_PATHS;

/**
 * Read a file and return its contents, or null if missing / unreadable.
 * Matches the contract every existing `readLog` injection expects.
 */
export function safeReadFile(filePath: string): string | null {
  try {
    if (!fs.existsSync(filePath)) return null;
    return fs.readFileSync(filePath, 'utf-8');
  } catch {
    return null;
  }
}

/**
 * Read only the last `maxBytes` of a file as utf-8 (or the whole file if smaller),
 * or null if missing / unreadable. The #3067 tail pattern (index-all-sources-deps
 * `readTail`), lifted here for request-path callers: reading a multi-hundred-MB log
 * synchronously on the event loop is the freeze (#3406 — /context/spine read the
 * whole 535MB chorus.log per request). Consumers that scan from the end and tolerate
 * a partial leading line (parseTailEvents) get the same recent events for ~0 cost.
 */
export function readFileTail(filePath: string, maxBytes: number): string | null {
  try {
    if (!fs.existsSync(filePath)) return null;
    const size = fs.statSync(filePath).size;
    if (size <= maxBytes) return fs.readFileSync(filePath, 'utf-8');
    const fd = fs.openSync(filePath, 'r');
    try {
      const buf = Buffer.alloc(maxBytes);
      fs.readSync(fd, buf, 0, maxBytes, size - maxBytes);
      return buf.toString('utf-8');
    } finally {
      fs.closeSync(fd);
    }
  } catch {
    return null;
  }
}

/**
 * Read a log file and return its non-empty lines, or [] if missing.
 */
export function readLogLines(filePath: string): string[] {
  const raw = safeReadFile(filePath);
  if (raw === null) return [];
  return raw.split('\n').filter((l) => l.length > 0);
}
