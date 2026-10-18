// @domain: cards
// #4598: Read live /cws pipeline status for a card from chorus-werk-status without recomputing.

import { execFile } from 'child_process';
import fs from 'fs';
import path from 'path';

export interface CardPipelineStatus {
  cardId: number;
  output: string;
  isLive: boolean;
  status: 'running' | 'waiting' | 'failed' | 'succeeded' | 'landed' | 'no-runs' | 'unmeasured' | 'unknown';
  step?: string;
  failedStep?: string;
  failedReason?: string;
  unmeasuredReason?: string;
}

function extractFailureFromLine(line: string): { failedStep?: string; failedReason?: string } | null {
  const parts = line.split('—');
  const left = parts[0] || '';
  const failedIdx = left.indexOf('failed');
  if (failedIdx === -1) return null;

  const afterFailed = left.slice(failedIdx + 6).trim();
  const words = afterFailed.split(/\s+/).filter(Boolean);
  const rawStep = words[0] === 'at' ? words[1] : words[0];
  if (!rawStep) return null;

  const failedReason = parts.length > 1 ? parts.slice(1).join('—').trim() : undefined;
  return { failedStep: rawStep, failedReason };
}

function extractFailure(output: string): { failedStep?: string; failedReason?: string } {
  const failureLines = output.split('\n').filter((l) => l.includes('failed'));

  // First check lines with '—' containing step and reason
  for (const line of failureLines) {
    if (line.includes('—')) {
      const res = extractFailureFromLine(line);
      if (res?.failedStep) return res;
    }
  }

  // Fallback to any line indicating failure
  for (const line of failureLines) {
    const res = extractFailureFromLine(line);
    if (res?.failedStep) return res;
  }

  return {};
}

function extractRunningStep(firstLine: string, output: string): string | undefined {
  if (firstLine.includes('·')) {
    const dots = firstLine.split('·').map((s) => s.trim());
    const lastDot = dots[dots.length - 1];
    if (lastDot && !lastDot.toLowerCase().includes('running')) {
      return lastDot;
    }
  }
  const lines = output.split('\n');
  for (const line of lines) {
    const idx = line.indexOf('running');
    if (idx !== -1) {
      const rest = line.slice(idx + 7).trim();
      const token = rest.split(/\s+/)[0];
      if (token && token.length > 0) return token;
    }
  }
  return undefined;
}

function derivePipelineStatus(firstLine: string, output: string, hasFailure: boolean): CardPipelineStatus['status'] {
  const lowerFirst = firstLine.toLowerCase();
  if (lowerFirst.includes('no runs') || lowerFirst.includes('no run yet')) return 'no-runs';
  if (lowerFirst.includes('landed')) return 'landed';
  if (lowerFirst.includes('running')) return 'running';
  if (lowerFirst.includes('waiting')) return 'waiting';
  if (lowerFirst.includes('failed')) return 'failed';
  if (lowerFirst.includes('succeeded')) return 'succeeded';
  if (hasFailure && !output.toLowerCase().includes('running')) return 'failed';
  return 'unknown';
}

export function parsePipelineOutput(cardId: number, output: string): CardPipelineStatus {
  const lines = output.split('\n');
  const firstLine = lines[0] || '';

  const { failedStep, failedReason } = extractFailure(output);
  const status = derivePipelineStatus(firstLine, output, Boolean(failedStep || failedReason));
  let step: string | undefined;

  if (status === 'running') {
    step = extractRunningStep(firstLine, output);
  } else if (status === 'waiting') {
    step = 'skill-werk-go';
  }

  const isLive = status === 'running' || status === 'waiting';

  return {
    cardId,
    output,
    isLive,
    status,
    step,
    failedStep,
    failedReason,
  };
}

export function resolveChorusWerkStatusBin(): string | null {
  // eslint-disable-next-line security/detect-non-literal-fs-filename
  if (process.env.CHORUS_WERK_STATUS_BIN && fs.existsSync(process.env.CHORUS_WERK_STATUS_BIN)) {
    return process.env.CHORUS_WERK_STATUS_BIN;
  }
  const relativePath = path.resolve(__dirname, '../../../../platform/scripts/chorus-werk-status');
  if (fs.existsSync(relativePath)) return relativePath;

  const canonicalPath = '/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-werk-status';
  if (fs.existsSync(canonicalPath)) return canonicalPath;

  const chorusBinPath = '/Users/jeffbridwell/.chorus/bin/chorus-werk-status';
  if (fs.existsSync(chorusBinPath)) return chorusBinPath;

  return null;
}

const lastGoodOutputByCard = new Map<number, string>();

function isCwsFailure(text: string): boolean {
  const lower = text.toLowerCase();
  return (
    lower.startsWith('error:') ||
    lower.includes('traceback (most recent call last):') ||
    lower.includes('syntaxerror:') ||
    lower.includes('permission denied') ||
    lower.includes('timed out')
  );
}

function extractCleanReason(raw: string): string {
  const lines = raw.split('\n').map((l) => l.trim()).filter(Boolean);
  const excLine = lines.slice().reverse().find((l) => l.includes('Error:') || l.includes('Exception:'));
  if (excLine) return excLine;
  return lines[0] || 'unmeasured';
}

function buildUnmeasuredStatus(cardId: number, rawReason: string): CardPipelineStatus {
  const cleanReason = extractCleanReason(rawReason);
  const lastGood = lastGoodOutputByCard.get(cardId);
  if (lastGood) {
    const parsed = parsePipelineOutput(cardId, lastGood);
    return {
      ...parsed,
      isLive: false,
      status: 'unmeasured',
      unmeasuredReason: cleanReason,
      output: `${lastGood}\n\n[unmeasured: ${cleanReason} — showing last good output]`,
    };
  }
  return {
    cardId,
    output: `unmeasured (${cleanReason})`,
    isLive: false,
    status: 'unmeasured',
    unmeasuredReason: cleanReason,
  };
}

export async function fetchCardPipeline(
  cardId: number,
  options?: {
    cwsBin?: string;
    runCws?: (cardId: number) => Promise<string>;
  }
): Promise<CardPipelineStatus> {
  if (options?.runCws) {
    try {
      const out = await options.runCws(cardId);
      if (isCwsFailure(out)) {
        return buildUnmeasuredStatus(cardId, out);
      }
      lastGoodOutputByCard.set(cardId, out);
      return parsePipelineOutput(cardId, out);
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      return buildUnmeasuredStatus(cardId, msg);
    }
  }

  const binPath = options?.cwsBin || resolveChorusWerkStatusBin();
  if (!binPath) {
    return {
      cardId,
      output: 'chorus-werk-status script not found',
      isLive: false,
      status: 'unknown',
    };
  }

  return new Promise<CardPipelineStatus>((resolve) => {
    execFile('python3', [binPath, String(cardId)], { timeout: 10000, env: process.env }, (err, stdout, stderr) => {
      if (err) {
        const execErr = err as { killed?: boolean; signal?: string; message?: string };
        const isTimeout = Boolean(execErr.killed || execErr.signal === 'SIGTERM' || execErr.message?.includes('timed out'));
        const reason = isTimeout ? 'Timed out after 10s' : (err.message || stderr || 'Failed to read status');
        return resolve(buildUnmeasuredStatus(cardId, reason));
      }
      const out = stdout || stderr || 'No output';
      if (isCwsFailure(out)) {
        return resolve(buildUnmeasuredStatus(cardId, out));
      }
      lastGoodOutputByCard.set(cardId, out);
      resolve(parsePipelineOutput(cardId, out));
    });
  });
}
