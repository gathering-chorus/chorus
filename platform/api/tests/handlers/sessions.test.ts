/**
 * @test-type: unit
 *
 * sessions handlers — unit tests (#2173 AC4).
 *
 * Three handlers sharing a deps object. Tests cover the branches that
 * matter: valid session id with session → 200; invalid id format → 400;
 * valid id but session missing → 404; text vs json content-type on the
 * log handler.
 *
 * #3559: these went stale while the suite was excluded from the hermetic
 * default (audit-flagged "review, not renamed"). #3039 made the three handlers
 * ASYNC (Promise<FetchResult>), but the tests still called them synchronously,
 * so `r.status` was `undefined` against every expectation. Fixed by awaiting.
 * Also: the handlers now coerce a non-string id to '' (→ invalid → 400), so the
 * old "non-string coerced via String() → 404" expectation was stale and is
 * corrected to 400. The handlers are correct; the tests had drifted.
 *
 * CLASSIFICATION (#3559, Silas-confirmed): fully INJECTED deps, touches no live
 * sessions store → hermetic in substance. Promoted into the HERMETIC project
 * (no longer excluded in jest.config). @test-type: unit.
 */

import {
  fetchSessionList,
  fetchSessionById,
  fetchSessionLog,
  type SessionsDeps,
} from '../../src/handlers/sessions';

function deps(overrides: Partial<SessionsDeps> = {}): SessionsDeps {
  return {
    listSessions: () => [{ id: 'a' }, { id: 'b' }],
    getSession: (id) => (id === 'known' ? { id, events: [] } : null),
    getSessionLog: (id) => (id === 'known' ? 'log content' : null),
    isValidSessionId: (id) => /^[a-z0-9_-]+$/.test(id),
    ...overrides,
  };
}

describe('fetchSessionList', () => {
  test('returns list from deps.listSessions', async () => {
    const r = await fetchSessionList(deps());
    expect(r.status).toBe(200);
    expect(r.body).toEqual([{ id: 'a' }, { id: 'b' }]);
  });

  test('listSessions throw maps to 500', async () => {
    const r = await fetchSessionList(deps({
      listSessions: () => { throw new Error('db locked'); },
    }));
    expect(r.status).toBe(500);
    expect(r.body).toEqual({ error: 'db locked' });
  });

  test('non-Error throw stringifies', async () => {
    const r = await fetchSessionList(deps({
      listSessions: () => { throw 'oops'; },
    }));
    expect(r.status).toBe(500);
    expect(r.body).toEqual({ error: 'oops' });
  });
});

describe('fetchSessionById', () => {
  test('known id returns 200 with session body', async () => {
    const r = await fetchSessionById(deps(), 'known');
    expect(r.status).toBe(200);
    expect(r.body).toEqual({ id: 'known', events: [] });
  });

  test('invalid id format returns 400', async () => {
    const r = await fetchSessionById(deps(), 'bad id with spaces');
    expect(r.status).toBe(400);
    expect(r.body).toEqual({ error: 'invalid session id' });
  });

  test('empty id returns 400', async () => {
    const r = await fetchSessionById(deps(), '');
    expect(r.status).toBe(400);
  });

  test('valid id but session not found returns 404', async () => {
    const r = await fetchSessionById(deps(), 'unknown');
    expect(r.status).toBe(404);
    expect(r.body).toEqual({ error: 'session not found' });
  });

  test('non-string id is coerced to empty → invalid → 400', async () => {
    // #3039 handler coerces a non-string id to '' (typeof check), which fails
    // isValidSessionId → 400. (Was previously asserted as 404 under an older
    // String()-coercion path that no longer exists.)
    const r = await fetchSessionById(deps(), 12345);
    expect(r.status).toBe(400);
    expect(r.body).toEqual({ error: 'invalid session id' });
  });
});

describe('fetchSessionLog', () => {
  test('known id returns 200 text/plain log', async () => {
    const r = await fetchSessionLog(deps(), 'known');
    expect(r.status).toBe(200);
    expect(r.body).toBe('log content');
    expect(r.contentType).toBe('text/plain');
  });

  test('invalid id returns 400 json', async () => {
    const r = await fetchSessionLog(deps(), '!!!');
    expect(r.status).toBe(400);
    expect(r.body).toEqual({ error: 'invalid session id' });
    expect(r.contentType).toBeUndefined();
  });

  test('valid id but log missing returns 404', async () => {
    const r = await fetchSessionLog(deps(), 'unknown');
    expect(r.status).toBe(404);
    expect(r.body).toEqual({ error: 'log not found' });
    expect(r.contentType).toBeUndefined();
  });
});
