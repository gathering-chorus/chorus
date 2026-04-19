/**
 * session-replay — unit tests (#2167).
 *
 * Target: 80%+ on src/session-replay.ts. Uses a tempdir via SESSIONS_DIR env.
 */

import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';

const TMP = fs.mkdtempSync(path.join(os.tmpdir(), 'session-replay-test-'));
process.env.SESSIONS_DIR = TMP;

import { listSessions, getSession, getSessionLog, isValidSessionId } from '../src/session-replay';

function load() {
  return { listSessions, getSession, getSessionLog, isValidSessionId };
}

afterAll(() => {
  try { fs.rmSync(TMP, { recursive: true, force: true }); } catch {}
});

function clear() {
  for (const f of fs.readdirSync(TMP)) fs.unlinkSync(path.join(TMP, f));
}

function makeSessionFile(id: string, meta: any, events: any[] = []) {
  fs.writeFileSync(path.join(TMP, `${id}.json`), JSON.stringify({ meta, events }));
}

describe('session-replay — isValidSessionId', () => {
  test('accepts ses_<digits>_<hex> format', () => {
    const { isValidSessionId } = load();
    expect(isValidSessionId('ses_1234_abc123')).toBe(true);
    expect(isValidSessionId('ses_0_a')).toBe(true);
  });

  test('rejects non-matching strings', () => {
    const { isValidSessionId } = load();
    expect(isValidSessionId('bad')).toBe(false);
    expect(isValidSessionId('session_1_abc')).toBe(false);
    expect(isValidSessionId('ses_abc_abc')).toBe(false);
    expect(isValidSessionId('')).toBe(false);
    expect(isValidSessionId('../traversal')).toBe(false);
  });
});

describe('session-replay — listSessions', () => {
  beforeEach(() => { clear(); jest.resetModules(); });

  test('empty sessions dir returns empty list', () => {
    const { listSessions } = load();
    expect(listSessions()).toEqual({ sessions: [] });
  });

  test('returns sessions sorted by lastActivity descending', () => {
    makeSessionFile('ses_1_aaa', {
      sessionId: 'ses_1_aaa', startTime: '2026-04-10T09:00:00Z',
      lastActivity: '2026-04-10T10:00:00Z', pages: ['/'], eventCount: 5,
    });
    makeSessionFile('ses_2_bbb', {
      sessionId: 'ses_2_bbb', startTime: '2026-04-15T09:00:00Z',
      lastActivity: '2026-04-15T10:00:00Z', pages: ['/borg'], eventCount: 9,
    });
    makeSessionFile('ses_3_ccc', {
      sessionId: 'ses_3_ccc', startTime: '2026-04-17T09:00:00Z',
      lastActivity: '2026-04-17T10:00:00Z', pages: ['/'], eventCount: 2,
    });
    const { listSessions } = load();
    const ids = listSessions().sessions.map((s: any) => s.sessionId);
    expect(ids).toEqual(['ses_3_ccc', 'ses_2_bbb', 'ses_1_aaa']);
  });

  test('skips files older than 7 days (MAX_SESSION_AGE_MS)', () => {
    makeSessionFile('ses_old_x', {
      sessionId: 'ses_old_x', startTime: '2020-01-01T00:00:00Z',
      lastActivity: '2020-01-01T00:00:00Z', pages: [], eventCount: 0,
    });
    // Set mtime to 91 days ago (MAX_SESSION_AGE_MS = 90 days per #2162)
    const oldPath = path.join(TMP, 'ses_old_x.json');
    const oldTime = new Date(Date.now() - 91 * 24 * 60 * 60 * 1000);
    fs.utimesSync(oldPath, oldTime, oldTime);

    makeSessionFile('ses_new_y', {
      sessionId: 'ses_new_y', startTime: new Date().toISOString(),
      lastActivity: new Date().toISOString(), pages: [], eventCount: 1,
    });

    const { listSessions } = load();
    const ids = listSessions().sessions.map((s: any) => s.sessionId);
    expect(ids).toEqual(['ses_new_y']);
  });

  test('ignores non-JSON files in the directory', () => {
    fs.writeFileSync(path.join(TMP, 'README.txt'), 'not a session');
    fs.writeFileSync(path.join(TMP, 'loose.log'), 'log content');
    makeSessionFile('ses_1_x', {
      sessionId: 'ses_1_x', startTime: '2026-04-17T09:00:00Z',
      lastActivity: '2026-04-17T10:00:00Z', pages: [], eventCount: 1,
    });
    const { listSessions } = load();
    expect(listSessions().sessions.map((s: any) => s.sessionId)).toEqual(['ses_1_x']);
  });

  test('tolerates malformed JSON files (skip)', () => {
    fs.writeFileSync(path.join(TMP, 'ses_bad_x.json'), '{ not valid');
    makeSessionFile('ses_good_y', {
      sessionId: 'ses_good_y', startTime: '2026-04-17T09:00:00Z',
      lastActivity: '2026-04-17T10:00:00Z', pages: [], eventCount: 1,
    });
    const { listSessions } = load();
    expect(listSessions().sessions.map((s: any) => s.sessionId)).toEqual(['ses_good_y']);
  });

  test('session file without meta is silently skipped', () => {
    fs.writeFileSync(path.join(TMP, 'ses_meta_x.json'), JSON.stringify({ events: [] }));
    const { listSessions } = load();
    expect(listSessions().sessions).toEqual([]);
  });

  test('returns empty when sessions dir does not exist', () => {
    process.env.SESSIONS_DIR = path.join(TMP, 'nonexistent-subdir');
    jest.resetModules();
    const { listSessions } = load();
    expect(listSessions()).toEqual({ sessions: [] });
    process.env.SESSIONS_DIR = TMP;
  });
});

describe('session-replay — getSession', () => {
  beforeEach(() => { clear(); jest.resetModules(); });

  test('returns parsed payload for a valid id with an existing file', () => {
    makeSessionFile('ses_99_zzz', {
      sessionId: 'ses_99_zzz', startTime: 's', lastActivity: 'e',
      pages: ['/x'], eventCount: 3,
    }, [{ type: 'snapshot' }]);
    const { getSession } = load();
    const s = getSession('ses_99_zzz');
    expect(s).not.toBeNull();
    expect(s!.meta.sessionId).toBe('ses_99_zzz');
    expect(s!.events).toHaveLength(1);
  });

  test('returns null for invalid id', () => {
    const { getSession } = load();
    expect(getSession('bad-id')).toBeNull();
  });

  test('returns null when file does not exist', () => {
    const { getSession } = load();
    expect(getSession('ses_404_nope')).toBeNull();
  });

  test('returns null when file is malformed JSON', () => {
    fs.writeFileSync(path.join(TMP, 'ses_bad_y.json'), '{ partial');
    const { getSession } = load();
    expect(getSession('ses_bad_y')).toBeNull();
  });
});

describe('session-replay — getSessionLog', () => {
  beforeEach(() => { clear(); jest.resetModules(); });

  test('returns log content when file exists', () => {
    fs.writeFileSync(path.join(TMP, 'ses_1_a.log'), 'log line one\nlog line two');
    const { getSessionLog } = load();
    expect(getSessionLog('ses_1_a')).toBe('log line one\nlog line two');
  });

  test('returns null for invalid id', () => {
    const { getSessionLog } = load();
    expect(getSessionLog('nope')).toBeNull();
  });

  test('returns null when log file absent', () => {
    const { getSessionLog } = load();
    expect(getSessionLog('ses_404_abc')).toBeNull();
  });
});
