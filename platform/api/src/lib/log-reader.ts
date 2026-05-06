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

export const CHORUS_ROOT: string =
  process.env.CHORUS_ROOT || path.join(os.homedir(), 'CascadeProjects/chorus');

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
 * Read a log file and return its non-empty lines, or [] if missing.
 */
export function readLogLines(filePath: string): string[] {
  const raw = safeReadFile(filePath);
  if (raw === null) return [];
  return raw.split('\n').filter((l) => l.length > 0);
}
