/**
 * #3060 — freshness worker thread entrypoint. Runs the recompute in its own
 * thread so the index COUNT never runs on chorus-api's serving event loop.
 * Reply shape matches worker-pool: { id, rows: [result] } or { id, error }.
 */
import { parentPort } from 'node:worker_threads';
import os from 'node:os';
import path from 'node:path';
import { computeFreshness } from './freshness-compute';

const dbPath = process.env.CHORUS_DB_PATH || path.join(os.homedir(), '.chorus', 'index.db');
const spineLogPath = path.join(os.homedir(), '.chorus', 'chorus.log');

parentPort?.on('message', (msg: { id: number }) => {
  try {
    parentPort?.postMessage({ id: msg.id, rows: [computeFreshness(dbPath, spineLogPath)] });
  } catch (e) {
    parentPort?.postMessage({ id: msg.id, error: (e as Error).message });
  }
});
