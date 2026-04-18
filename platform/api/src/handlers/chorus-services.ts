/**
 * GET /api/chorus/services — LaunchAgent status (#1485, extracted #2189).
 *
 * Lists com.chorus.* and com.gathering.* LaunchAgents via `launchctl list`.
 * For running services (PID present), enriches with RSS via `ps -o pid,rss`.
 * Returns service array + counts + total RSS in MB.
 */
import { execFile as execFileCb } from 'child_process';
import { promisify } from 'util';

export type ExecFileFn = (
  cmd: string,
  args: string[],
  opts?: { timeout?: number },
) => Promise<{ stdout: string; stderr: string }>;

const defaultExecFile: ExecFileFn = promisify(execFileCb) as unknown as ExecFileFn;

export interface ServicesDeps {
  execFile?: ExecFileFn;
}

export interface ServiceEntry {
  label: string;
  pid: number | null;
  status: number;
  rss_mb: number | null;
}

export interface ServicesResult {
  status: number;
  body:
    | { services: ServiceEntry[]; running: number; total: number; total_rss_mb?: number }
    | { error: string };
}

export async function fetchServices({
  execFile = defaultExecFile,
}: ServicesDeps = {}): Promise<ServicesResult> {
  let lcOut: string;
  try {
    const r = await execFile('launchctl', ['list'], { timeout: 10000 });
    lcOut = r.stdout;
  } catch {
    return { status: 500, body: { error: 'launchctl list failed' } };
  }

  const base: Array<{ label: string; pid: number | null; status: number }> = [];
  for (const line of lcOut.trim().split('\n').slice(1)) {
    const parts = line.split('\t');
    if (parts.length < 3) continue;
    const label = parts[2];
    if (!label.startsWith('com.chorus.') && !label.startsWith('com.gathering.')) continue;
    base.push({
      label,
      pid: parts[0] === '-' ? null : parseInt(parts[0], 10),
      status: parseInt(parts[1], 10),
    });
  }

  const pids = base.filter((s) => s.pid !== null).map((s) => s.pid as number);
  if (pids.length === 0) {
    return {
      status: 200,
      body: { services: base.map((s) => ({ ...s, rss_mb: null })), running: 0, total: base.length },
    };
  }

  const rssMap = new Map<number, number>();
  try {
    const psRes = await execFile('ps', ['-o', 'pid=,rss=', '-p', pids.join(',')], { timeout: 5000 });
    for (const line of psRes.stdout.trim().split('\n')) {
      const [pidStr, rssStr] = line.trim().split(/\s+/);
      if (pidStr && rssStr) {
        rssMap.set(parseInt(pidStr, 10), Math.round(parseInt(rssStr, 10) / 1024));
      }
    }
  } catch {
    // ps unavailable — rss_mb stays null
  }

  const enriched: ServiceEntry[] = base.map((s) => ({
    ...s,
    rss_mb: s.pid !== null ? rssMap.get(s.pid) ?? null : null,
  }));

  const running = enriched.filter((s) => s.pid !== null).length;
  const totalRss = enriched.reduce((sum, s) => sum + (s.rss_mb || 0), 0);

  return {
    status: 200,
    body: {
      services: enriched,
      running,
      total: enriched.length,
      total_rss_mb: totalRss,
    },
  };
}
