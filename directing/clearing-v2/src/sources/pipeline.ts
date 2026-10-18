// @domain: cards
// #4598: Read live /cws pipeline status for a card from chorus-werk-status without recomputing.

import { execFile } from 'child_process';
import fs from 'fs';
import path from 'path';

export interface CardPipelineStatus {
  cardId: number;
  output: string;
  isLive: boolean;
  status: 'running' | 'waiting' | 'failed' | 'succeeded' | 'landed' | 'no-runs' | 'unknown';
  step?: string;
  failedStep?: string;
  failedReason?: string;
}

export function parsePipelineOutput(cardId: number, output: string): CardPipelineStatus {
  const lines = output.split('\n');
  const firstLine = lines[0] || '';

  let failedStep: string | undefined;
  let failedReason: string | undefined;
  let step: string | undefined;
  let status: CardPipelineStatus['status'] = 'unknown';

  // Look for failed step and reason in the table or output
  // e.g.: "cicd  1  10-17 20:35  20:35   failed      skill-werk-commit  — werk-commit: git rev-parse..."
  // or: "failed skill-werk-commit — reason"
  const tableMatch = output.match(/^\s*\S+\s+\d+\s+[\d:-]+\s+[\d:-]+\s+failed\s+([\w-]+)(?:\s*—\s*([^\n]+))?/m);
  if (tableMatch) {
    failedStep = tableMatch[1];
    failedReason = tableMatch[2]?.trim();
  } else {
    const failedDashMatch = output.match(/\bfailed\s+([\w-]+)\s*—\s*([^\n]+)/i);
    if (failedDashMatch) {
      failedStep = failedDashMatch[1];
      failedReason = failedDashMatch[2]?.trim();
    } else {
      const generalMatch = output.match(/\bfailed\s+(?:at\s+)?([\w-]+)/i);
      if (generalMatch && generalMatch[1] !== 'at') {
        failedStep = generalMatch[1];
      }
    }
  }

  // Determine overall status from first line / dagu line
  if (/no runs|no run yet/i.test(firstLine)) {
    status = 'no-runs';
  } else if (/\blanded\b/i.test(firstLine)) {
    status = 'landed';
  } else if (/\brunning\b/i.test(firstLine)) {
    status = 'running';
    const stepMatch = firstLine.match(/running\s*(?:·\s*[^·]+·\s*([\w-]+))?/i);
    if (stepMatch && stepMatch[1]) {
      step = stepMatch[1];
    } else {
      const runningTableMatch = output.match(/running\s+([\w-]+)/i);
      if (runningTableMatch) step = runningTableMatch[1];
    }
  } else if (/\bwaiting\b/i.test(firstLine)) {
    status = 'waiting';
    step = 'skill-werk-go';
  } else if (/\bfailed\b/i.test(firstLine)) {
    status = 'failed';
  } else if (/\bsucceeded\b/i.test(firstLine)) {
    status = 'succeeded';
  } else if ((failedStep || failedReason) && !/\brunning\b/i.test(output)) {
    status = 'failed';
  }

  // #4598 AC: Motion and auto-refresh while live
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
      return parsePipelineOutput(cardId, out);
    } catch (e: any) {
      return parsePipelineOutput(cardId, `Error running chorus-werk-status: ${e.message}`);
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
      const out = stdout || stderr || (err ? `Error: ${err.message}` : 'No output');
      resolve(parsePipelineOutput(cardId, out));
    });
  });
}
