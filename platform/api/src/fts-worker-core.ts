/**
 * #3086 — pure message logic for the FTS worker thread. Kept separate from the
 * worker_threads glue (fts-worker.ts) so it's unit-testable without a real thread.
 * Runs the SAME runFtsQueryOnDb the in-process path uses → parity by construction.
 */
import type Database from 'better-sqlite3';
import { runFtsQueryOnDb } from './lib/fts-query';

export interface FtsRequest {
  id: number;
  q: string;
  fetchLimit: number;
  role?: string;
  mode: string;
}

export type FtsReply = { id: number; rows: unknown[] } | { id: number; error: string };

/**
 * Run one FTS request against the worker's db connection. Any failure becomes an
 * error reply rather than a throw, so a bad message can never crash the worker
 * (the pool still guards process-level crash + timeout separately).
 */
export function handleFtsMessage(db: Database.Database, msg: FtsRequest): FtsReply {
  const id = msg && typeof msg.id === 'number' ? msg.id : -1;
  try {
    const rows = runFtsQueryOnDb(db, msg.q, msg.fetchLimit, msg.role, msg.mode);
    return { id, rows };
  } catch (e) {
    return { id, error: e instanceof Error ? e.message : String(e) };
  }
}
