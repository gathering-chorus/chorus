/**
 * #3086 — the worker's message logic, tested as a pure function (no real thread).
 * handleFtsMessage runs the SAME runFtsQueryOnDb the in-process path uses, so a
 * valid request returns identical rows (parity), and a bad message becomes an
 * error reply instead of crashing the worker (part of "errors surface, not hang").
 */
import { handleFtsMessage } from '../src/fts-worker-core';
import { runFtsQueryOnDb } from '../src/lib/fts-query';
import { makeFtsDb } from './helpers/fts-fixture';

describe('handleFtsMessage (#3086)', () => {
  test('returns the matching rows for a valid request (parity)', () => {
    const db = makeFtsDb();
    const reply = handleFtsMessage(db, { id: 7, q: 'search', fetchLimit: 10, mode: 'fts' });
    expect(reply.id).toBe(7);
    expect(reply).toHaveProperty('rows');
    const direct = runFtsQueryOnDb(db, 'search', 10, undefined, 'fts');
    expect(reply).toEqual({ id: 7, rows: direct });
    db.close();
  });

  test('turns an unexpected failure into an error reply, never throws', () => {
    const db = makeFtsDb();
    const reply = handleFtsMessage(db, undefined as never);
    expect(reply).toHaveProperty('error');
    db.close();
  });
});
