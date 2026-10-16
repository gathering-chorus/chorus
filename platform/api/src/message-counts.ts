// #4556 — the health cache's two message counts, run in a worker thread.
//
// Measured 2026-10-16 (CPU profile of chorus-api through three freeze alerts):
// the 30 s health-cache refresh spent 3.97 s and 1.05 s inside a synchronous
// better-sqlite3 .get on index.db, on the event loop, while the same queries
// run alone took 0.01 s. A worker thread does the same reads; the loop only
// awaits the answer, so a slow or locked read no longer stops every request.
import { Worker } from 'node:worker_threads';
import { MIN_EMBED_LENGTH } from './embed-floor';

export interface MessageCounts {
  dbRows: number;
  /** null when the embedded column does not exist yet. */
  unembedded: number | null;
}

// Runs inside the worker. Plain JS: the worker evaluates it as a string.
const WORKER_SOURCE = `
const { parentPort, workerData } = require('node:worker_threads');
const Database = require(workerData.sqliteModule);
let db;
try {
  db = new Database(workerData.dbPath, { readonly: true });
  const dbRows = db.prepare('SELECT COUNT(*) as cnt FROM messages').get().cnt;
  let unembedded = null;
  try {
    unembedded = db.prepare('SELECT COUNT(*) as cnt FROM messages WHERE embedded = 0 AND LENGTH(content) >= ?').get(workerData.floor).cnt;
  } catch (e) { /* column may not exist yet */ }
  parentPort.postMessage({ ok: true, dbRows, unembedded });
} catch (e) {
  parentPort.postMessage({ ok: false, error: String(e && e.message || e) });
} finally {
  try { db && db.close(); } catch (e) { /* closing a failed open */ }
}
`;

export function countMessagesInWorker(dbPath: string, timeoutMs = 20_000): Promise<MessageCounts> {
  return new Promise((resolve, reject) => {
    const worker = new Worker(WORKER_SOURCE, {
      eval: true,
      workerData: { dbPath, floor: MIN_EMBED_LENGTH, sqliteModule: require.resolve('better-sqlite3') },
    });
    const timer = setTimeout(() => {
      void worker.terminate();
      reject(new Error(`message counts took longer than ${timeoutMs} ms`));
    }, timeoutMs);
    worker.once('message', (m: { ok: boolean; dbRows?: number; unembedded?: number | null; error?: string }) => {
      clearTimeout(timer);
      void worker.terminate();
      if (m.ok) resolve({ dbRows: m.dbRows ?? 0, unembedded: m.unembedded ?? null });
      else reject(new Error(m.error ?? 'message counts failed'));
    });
    worker.once('error', (err) => { clearTimeout(timer); reject(err); });
  });
}
