/**
 * #3060 — freshness worker thread entrypoint. Runs the recompute in its own
 * thread so the index COUNT never runs on chorus-api's serving event loop.
 * Reply shape matches worker-pool: { id, rows: [result] } or { id, error }.
 */
import { parentPort } from 'node:worker_threads';
import os from 'node:os';
import path from 'node:path';
import { computeFreshness, type FreshnessResult } from './freshness-compute';

export type FreshnessReply = { id: number; rows: [FreshnessResult] } | { id: number; error: string };

export function handleFreshnessMessage(msg: { id: number }, compute: () => FreshnessResult): FreshnessReply {
  try {
    return { id: msg.id, rows: [compute()] };
  } catch (e) {
    return { id: msg.id, error: (e as Error).message };
  }
}

const dbPath = process.env.CHORUS_DB_PATH || path.join(os.homedir(), '.chorus', 'index.db');
const spineLogPath = path.join(os.homedir(), '.chorus', 'chorus.log');

// parentPort is null on the main thread (tests import the handler only).
parentPort?.on('message', (msg: { id: number }) => {
  parentPort?.postMessage(handleFreshnessMessage(msg, () => computeFreshness(dbPath, spineLogPath)));
});
