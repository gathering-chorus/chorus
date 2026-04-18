/**
 * sessions handlers — unit tests (#2173 AC4).
 *
 * Three handlers sharing a deps object. Tests cover the branches that
 * matter: valid session id with session → 200; invalid id format → 400;
 * valid id but session missing → 404; text vs json content-type on the
 * log handler.
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
  test('returns list from deps.listSessions', () => {
    const r = fetchSessionList(deps());
    expect(r.status).toBe(200);
    expect(r.body).toEqual([{ id: 'a' }, { id: 'b' }]);
  });

  test('listSessions throw maps to 500', () => {
    const r = fetchSessionList(deps({
      listSessions: () => { throw new Error('db locked'); },
    }));
    expect(r.status).toBe(500);
    expect(r.body).toEqual({ error: 'db locked' });
  });

  test('non-Error throw stringifies', () => {
    const r = fetchSessionList(deps({
      listSessions: () => { throw 'oops'; },
    }));
    expect(r.status).toBe(500);
    expect(r.body).toEqual({ error: 'oops' });
  });
});

describe('fetchSessionById', () => {
  test('known id returns 200 with session body', () => {
    const r = fetchSessionById(deps(), 'known');
    expect(r.status).toBe(200);
    expect(r.body).toEqual({ id: 'known', events: [] });
  });

  test('invalid id format returns 400', () => {
    const r = fetchSessionById(deps(), 'bad id with spaces');
    expect(r.status).toBe(400);
    expect(r.body).toEqual({ error: 'invalid session id' });
  });

  test('empty id returns 400', () => {
    const r = fetchSessionById(deps(), '');
    expect(r.status).toBe(400);
  });

  test('valid id but session not found returns 404', () => {
    const r = fetchSessionById(deps(), 'unknown');
    expect(r.status).toBe(404);
    expect(r.body).toEqual({ error: 'session not found' });
  });

  test('non-string id is coerced via String()', () => {
    const r = fetchSessionById(deps(), 12345);
    // '12345' passes isValidSessionId regex, not found → 404
    expect(r.status).toBe(404);
  });
});

describe('fetchSessionLog', () => {
  test('known id returns 200 text/plain log', () => {
    const r = fetchSessionLog(deps(), 'known');
    expect(r.status).toBe(200);
    expect(r.body).toBe('log content');
    expect(r.contentType).toBe('text/plain');
  });

  test('invalid id returns 400 json', () => {
    const r = fetchSessionLog(deps(), '!!!');
    expect(r.status).toBe(400);
    expect(r.body).toEqual({ error: 'invalid session id' });
    expect(r.contentType).toBeUndefined();
  });

  test('valid id but log missing returns 404', () => {
    const r = fetchSessionLog(deps(), 'unknown');
    expect(r.status).toBe(404);
    expect(r.body).toEqual({ error: 'log not found' });
    expect(r.contentType).toBeUndefined();
  });
});
