/**
 * GET /api/chorus/disk — Disk usage summary (#1485, extracted #2189).
 *
 * Runs two shell calls:
 *   - diskutil info / → container total + free bytes
 *   - osascript (Finder) → free bytes including purgeable space
 * Finder free is preferred; falls back to container free if osascript fails.
 *
 * Dependency injection: execFile is promisified and overridable so the
 * handler is testable without spawning real processes.
 */
import { execFile as execFileCb } from 'child_process';
import { promisify } from 'util';

export type ExecFileFn = (
  cmd: string,
  args: string[],
  opts?: { timeout?: number },
) => Promise<{ stdout: string; stderr: string }>;

const defaultExecFile: ExecFileFn = promisify(execFileCb) as unknown as ExecFileFn;

export interface DiskDeps {
  execFile?: ExecFileFn;
}

export interface DiskResult {
  status: number;
  body: Record<string, unknown>;
}

export async function fetchDisk({ execFile = defaultExecFile }: DiskDeps = {}): Promise<DiskResult> {
  let diskStdout: string;
  try {
    const r = await execFile('/usr/sbin/diskutil', ['info', '/'], { timeout: 10000 });
    diskStdout = r.stdout;
  } catch (e) {
    return {
      status: 500,
      body: { error: 'diskutil info failed', detail: (e as Error).message },
    };
  }

  const extract = (label: string): string | null => {
    // eslint-disable-next-line security/detect-non-literal-regexp -- label comes from hardcoded call sites, not user input.
    const match = diskStdout.match(new RegExp(`${label}:\\s*(.+)`));
    return match ? match[1].trim() : null;
  };

  const totalSize = extract('Container Total Space');
  const containerFreeSize = extract('Container Free Space');

  const parseBytes = (s: string | null): number | null => {
    if (!s) return null;
    const m = s.match(/\((\d+)\s*Bytes\)/);
    return m ? parseInt(m[1], 10) : null;
  };

  const totalBytes = parseBytes(totalSize);
  const containerFreeBytes = parseBytes(containerFreeSize);

  let finderFreeBytes: number | null = null;
  try {
    const r = await execFile(
      '/usr/bin/osascript',
      ['-e', 'tell application "Finder" to get free space of startup disk'],
      { timeout: 5000 },
    );
    finderFreeBytes = Math.round(parseFloat(r.stdout.trim()));
  } catch {
    finderFreeBytes = null;
  }

  const freeBytes = finderFreeBytes ?? containerFreeBytes;
  const usedBytes = totalBytes && freeBytes !== null ? totalBytes - freeBytes : null;
  const usedPct = totalBytes && usedBytes !== null ? Math.round((usedBytes / totalBytes) * 100) : null;

  return {
    status: 200,
    body: {
      machine: 'Library',
      total: totalSize,
      free: containerFreeSize,
      total_bytes: totalBytes,
      container_free_bytes: containerFreeBytes,
      finder_free_bytes: finderFreeBytes,
      free_bytes: freeBytes,
      used_bytes: usedBytes,
      used_pct: usedPct,
      warning: usedPct !== null && usedPct >= 90,
      critical: usedPct !== null && usedPct >= 95,
    },
  };
}
