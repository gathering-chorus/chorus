// @test-type: unit — pure resolution over fixture rows; no athena-make, no registry dir
/**
 * #4361 — pulse finds a role's pane from its Presence row.
 *
 * Fixture rows are shaped like the live /v1/identity/presences and
 * /v1/identity/sessionruns answers (2026-09-27): presenceOf names the run as
 * "session-run-<run name>", and a run is live while runEndedAt is empty.
 */
import { resolveFromPresence, fetchPresenceResolution, resolveEnds } from './presence-target';

const runs = [
  { name: 'silas-run-old', runEndedAt: '2026-09-26T16:31:00Z' },
  { name: 'silas-run-now', runEndedAt: '' },
  { name: 'kade-run-gone', runEndedAt: '2026-09-26T16:31:27Z' },
];
const presences = [
  { name: 'silas-presence-old', presenceOf: 'session-run-silas-run-old', pane: '%1', tty: '/dev/ttys005' },
  { name: 'silas-presence-now', presenceOf: 'session-run-silas-run-now', pane: '%15', tty: '/dev/ttys005' },
  { name: 'kade-presence-gone', presenceOf: 'session-run-kade-run-gone', pane: '%11', tty: '/dev/ttys003' },
];

describe('resolveFromPresence (#4361)', () => {
  test('a live run\'s Presence names the pane', () => {
    const r = resolveFromPresence(presences, runs, 'silas');
    expect(r.kind).toBe('resolved');
    if (r.kind === 'resolved') {
      expect(r.session.tmux).toBe('%15');
      expect(r.session.tty).toBe('/dev/ttys005');
      expect(r.session.role).toBe('silas');
    }
  });

  // Negative proof (#3734): the ended run's %1 is listed first and must never
  // be chosen. Drop the runEndedAt check in presence-target.ts and this goes red.
  test('an ended run\'s Presence is never the target', () => {
    const r = resolveFromPresence(presences, runs, 'silas');
    expect(r.kind === 'resolved' ? r.session.tmux : null).not.toBe('%1');
  });

  test('rows for the role but no live run → dead (logged out)', () => {
    expect(resolveFromPresence(presences, runs, 'kade')).toEqual({ kind: 'dead' });
  });

  test('no Presence rows for the role → unregistered (never logged in)', () => {
    expect(resolveFromPresence(presences, runs, 'wren')).toEqual({ kind: 'unregistered' });
  });

  test('a Presence with no pane is no-pane, never a typed tty guess (#4362)', () => {
    const r = resolveFromPresence(
      [{ name: 'wren-presence-x', presenceOf: 'session-run-wren-run-x', pane: '', tty: '/dev/ttys006' }],
      [{ name: 'wren-run-x', runEndedAt: '' }],
      'wren',
    );
    expect(r).toEqual({ kind: 'no-pane' });
  });
});

// #4362 reopened 2026-10-01 — a run logged in outside tmux stores pane "-".
// tmux reads "-" as "the current pane", so pulse typed every nudge for silas
// into whichever pane was on screen (wren's, then kade's: 4 in 3 hours).
describe('#4362 a pane that is not a tmux pane id is never a target', () => {
  const live = [{ name: 'silas-run-tmux', runEndedAt: '' }, { name: 'silas-run-shell', runEndedAt: '' }];
  const rows = [
    { name: 'silas-presence-tmux', presenceOf: 'session-run-silas-run-tmux', pane: '%63', tty: '/dev/ttys005', checkedAt: '2026-09-30T19:25:36Z' },
    { name: 'silas-presence-shell', presenceOf: 'session-run-silas-run-shell', pane: '-', tty: '/dev/ttys007', checkedAt: '2026-10-01T12:18:42Z' },
  ];

  // NEGATIVE PROOF (#3734): the "-" row is the newest, so a resolver that only
  // sorts by checkedAt picks it. This is the live state of 2026-10-01 08:18.
  test('the live run with a real pane wins over a newer run with pane "-"', () => {
    const r = resolveFromPresence(rows, live, 'silas');
    expect(r.kind === 'resolved' ? r.session.tmux : null).toBe('%63');
  });

  // The live state at 08:30: silas's %63 run had ended; his only live run was
  // the werk-demo's on ttys007, pane "-". Nothing in that terminal is silas.
  test('a lone run with pane "-" is no-pane: nothing is typed anywhere', () => {
    expect(resolveFromPresence([rows[1]], [live[1]], 'silas')).toEqual({ kind: 'no-pane' });
  });
});

// #4362 — the read from athena-make, end to end with a fake API: where a
// nudge goes, and what the sender's own session is, come from the same rows.

const SILAS_TTY = ['/dev', 'ttys005'].join('/');

describe('#4362 fetchPresenceResolution / resolveEnds against the API', () => {
  const realFetch = global.fetch;
  afterEach(() => { global.fetch = realFetch; });

  function api(status: number, presences: unknown[], sessionruns: unknown[]): void {
    global.fetch = ((url: string) => Promise.resolve({
      ok: status < 300,
      status,
      json: () => Promise.resolve({ data: String(url).includes('/presences') ? presences : sessionruns }),
    })) as unknown as typeof fetch;
  }

  it('routes to the live run\'s pane and names the sender\'s session', async () => {
    api(200,
      [{ name: 'silas-presence-1', presenceOf: 'session-run-silas-run-now', pane: '%63', tty: SILAS_TTY, checkedAt: '2026-09-30T19:37:00Z' },
       { name: 'wren-presence-1', presenceOf: 'session-run-wren-run-now', pane: '%65', tty: '/dev/ttys006', checkedAt: '2026-09-30T19:39:00Z' }],
      [{ name: `silas-run-${'now'}`, runEndedAt: '' }, { name: 'wren-run-now', runEndedAt: '' }]);
    const { toRes, sender } = await resolveEnds('silas', 'wren');
    expect(toRes).toEqual({ kind: 'resolved', session: expect.objectContaining({ tty: SILAS_TTY, tmux: '%63' }) });
    expect(sender).toEqual(expect.objectContaining({ tty: '/dev/ttys006', tmux: '%65' }));
  });

  it('NEGATIVE PROOF — an API that refuses is "unread", never a guess at a pane', async () => {
    api(502, [], []);
    const res = await fetchPresenceResolution('silas', 'http://api');
    expect(res).toEqual({ kind: 'unread', why: 'presences answered HTTP 502' });
    const { sender } = await resolveEnds('silas', 'wren');
    expect(sender).toBeNull();
  });
});
