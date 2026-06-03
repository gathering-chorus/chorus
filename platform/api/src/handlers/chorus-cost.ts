/**
 * GET /api/chorus/cost — Cost summary (extracted #2189).
 *
 * Runs cost-report.sh with a period arg. Treats partial output (stdout
 * present + stderr present) as success with partial=true. Only returns
 * 500 when there's no stdout AND exec failed.
 */
import { execFile as execFileCb } from 'child_process';
import { promisify } from 'util';
import * as path from 'path';
import * as os from 'os';
import { CHORUS_ROOT } from '../lib/chorus-paths'; // #3197 — single root source

export type ExecFileFn = (
  cmd: string,
  args: string[],
  opts?: { timeout?: number; env?: NodeJS.ProcessEnv },
) => Promise<{ stdout: string; stderr: string }>;

const defaultExecFile: ExecFileFn = promisify(execFileCb) as unknown as ExecFileFn;

export interface CostDeps {
  execFile?: ExecFileFn;
  scriptPath?: string;
}

export interface CostResult {
  status: number;
  body: { period: string; output: string; partial: boolean } | { error: string; detail?: string };
}

function defaultScriptPath(): string {
  return path.join(CHORUS_ROOT, 'platform/scripts/cost-report.sh');
}

export async function fetchCost(
  period: string,
  { execFile = defaultExecFile, scriptPath = defaultScriptPath() }: CostDeps = {},
): Promise<CostResult> {
  let stdout = '';
  let stderr = '';
  let execErr: Error | null = null;
  try {
    const r = await execFile('bash', [scriptPath, period], {
      timeout: 15000,
      env: { ...process.env, HOME: os.homedir() },
    });
    stdout = r.stdout;
    stderr = r.stderr;
  } catch (e) {
    const err = e as Error & { stdout?: string; stderr?: string };
    stdout = err.stdout || '';
    stderr = err.stderr || '';
    execErr = err;
  }

  const output = stdout.trim();
  const errors = stderr.trim();

  if (!output && execErr) {
    return { status: 500, body: { error: 'cost-report.sh failed', detail: errors || execErr.message } };
  }

  return {
    status: 200,
    body: { period, output, partial: !!errors },
  };
}
