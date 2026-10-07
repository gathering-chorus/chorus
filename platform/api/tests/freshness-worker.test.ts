// @test-type: unit — the worker's message handler with an injected compute; no thread, no index.
// #3060 — freshness-worker.ts answers the runner in the worker-pool reply shape:
// { id, rows: [result] } on success, { id, error } when the recompute throws, so
// a failed count reaches the caller as a rejection instead of a hung request.

import { handleFreshnessMessage } from '../src/freshness-worker';

describe('freshness worker message handler (#3060)', () => {
  it('replies with the recompute result under the request id', () => {
    const reply = handleFreshnessMessage({ id: 7 }, () => ({ status: 200, body: { sources: [] } }));
    expect(reply).toEqual({ id: 7, rows: [{ status: 200, body: { sources: [] } }] });
  });

  it('a throwing recompute becomes an error reply, not a thrown exception', () => {
    const reply = handleFreshnessMessage({ id: 8 }, () => { throw new Error('database is locked'); });
    expect(reply).toEqual({ id: 8, error: 'database is locked' });
  });
});
