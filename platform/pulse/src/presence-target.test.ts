// @test-type: unit — pure resolution over fixture rows; no athena-make, no registry dir
/**
 * #4361 — pulse finds a role's pane from its Presence row.
 *
 * Fixture rows are shaped like the live /v1/identity/presences and
 * /v1/identity/sessionruns answers (2026-09-27): presenceOf names the run as
 * "session-run-<run name>", and a run is live while runEndedAt is empty.
 */
import { resolveFromPresence } from './presence-target';

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

  test('a Presence with no pane routes by tty', () => {
    const r = resolveFromPresence(
      [{ name: 'wren-presence-x', presenceOf: 'session-run-wren-run-x', pane: '', tty: '/dev/ttys006' }],
      [{ name: 'wren-run-x', runEndedAt: '' }],
      'wren',
    );
    expect(r.kind === 'resolved' ? r.session.tmux : 'x').toBeFalsy();
    expect(r.kind === 'resolved' ? r.session.tty : '').toBe('/dev/ttys006');
  });
});
