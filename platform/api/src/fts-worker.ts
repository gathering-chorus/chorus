/**
 * #3086 — FTS worker thread entrypoint. Opens the index read-only in its OWN
 * thread and answers FTS requests via handleFtsMessage, so the heavy synchronous
 * better-sqlite3 query (#3079: 5-7s on the 1.1GB db) never runs on chorus-api's
 * serving event loop.
 *
 * Thin glue (db open + parentPort wiring). The query logic lives in
 * fts-worker-core (unit-tested) and the dispatch/lifecycle in fts-worker-pool
 * (unit-tested); AC1/AC2 (off-loop, no eventloop.blocked) are verified at deploy,
 * consistent with the one-env policy. worker_threads inherits chorus-api's node,
 * so the better-sqlite3 ABI mismatch that bit #3085 cannot occur here.
 */
import { parentPort } from 'node:worker_threads';
import os from 'node:os';
import path from 'node:path';
import Database from 'better-sqlite3';
import { handleFtsMessage, type FtsRequest } from './fts-worker-core';

const dbPath = process.env.CHORUS_DB_PATH || path.join(os.homedir(), '.chorus', 'index.db');
const db = new Database(dbPath, { readonly: true, fileMustExist: true });
db.pragma('busy_timeout = 5000'); // align with #3073; readers never block the writer in WAL

parentPort?.on('message', (msg: FtsRequest) => {
  parentPort?.postMessage(handleFtsMessage(db, msg));
});
