/* eslint-disable security/detect-object-injection -- Indexing on validated metric keys. */
/**
 * GET /api/chorus/perf — Daily perf baseline summary (extracted #2189).
 *
 * Runs `perf-baseline.sh summary` and parses its tabular output:
 *   Perf Baseline — <date>
 *   Function       Today    Yesterday   Delta       Status
 *   fuseki:ping    123ms    120ms       ▲+2%        PASS
 *   ...
 *   N/M passed
 */
import { execFile as execFileCb } from 'child_process';
import { promisify } from 'util';
import * as path from 'path';
import * as os from 'os';

export type ExecFileFn = (
  cmd: string,
  args: string[],
  opts?: { timeout?: number },
) => Promise<{ stdout: string; stderr: string }>;

const defaultExecFile: ExecFileFn = promisify(execFileCb) as unknown as ExecFileFn;

export interface PerfDeps {
  execFile?: ExecFileFn;
  scriptPath?: string;
}

export interface PerfRow {
  function: string;
  today_ms: number;
  yesterday_ms: number;
  delta_pct: string;
  status: string;
}

export interface PerfResult {
  status: number;
  body:
    | { date: string | null; summary: string; passed: number; total: number; results: PerfRow[] }
    | { error: string; detail?: string };
}

function defaultScriptPath(): string {
  const root = process.env.CHORUS_ROOT || path.join(os.homedir(), 'CascadeProjects/chorus');
  return path.join(root, 'platform/scripts/perf-baseline.sh');
}

function parsePerfRow(line: string): PerfRow | null {
  const match = line.match(/^(\S+)\s+([\d,]+)ms\s+([\d,]+)ms\s+(.+?)\s+(PASS|FAIL)\s*$/);
  if (!match) return null;
  return {
    function: match[1],
    today_ms: parseInt(match[2].replace(/,/g, ''), 10),
    yesterday_ms: parseInt(match[3].replace(/,/g, ''), 10),
    delta_pct: match[4].trim(),
    status: match[5],
  };
}

function parsePerfRows(lines: string[], headerLine: number): PerfRow[] {
  if (headerLine < 0) return [];
  const results: PerfRow[] = [];
  for (let i = headerLine + 2; i < lines.length; i++) {
    const line = lines[i].trim();
    if (!line || /passed/.test(line)) continue;
    const row = parsePerfRow(line);
    if (row) results.push(row);
  }
  return results;
}

export async function fetchPerf({
  execFile = defaultExecFile,
  scriptPath = defaultScriptPath(),
}: PerfDeps = {}): Promise<PerfResult> {
  let stdout: string;
  try {
    const r = await execFile('bash', [scriptPath, 'summary'], { timeout: 30000 });
    stdout = r.stdout;
  } catch (e) {
    const err = e as Error & { stderr?: string };
    return { status: 500, body: { error: 'perf-baseline.sh failed', detail: (err.stderr || err.message || '').trim() } };
  }

  const lines = stdout.trim().split('\n');
  const headerLine = lines.findIndex((l) => /^Function\s/.test(l));
  const dateLine = lines.find((l) => /^Perf Baseline/.test(l));
  const summaryLine = lines.find((l) => /passed/.test(l));
  const date = dateLine?.replace('Perf Baseline — ', '').trim() || null;
  const results = parsePerfRows(lines, headerLine);
  const passed = results.filter((r) => r.status === 'PASS').length;
  const total = results.length;

  return {
    status: 200,
    body: {
      date,
      summary: summaryLine?.trim() || `${passed}/${total} passed`,
      passed,
      total,
      results,
    },
  };
}
