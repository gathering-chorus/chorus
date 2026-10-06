// @test-type: unit — injected role rows, mkdtemp scan dir, unreachable board API; brings its own world.
// @card: #4432
// @owner: silas
/**
 * #4432 — Jeff 2026-10-06: tiles are logged-in principals, not sessions, but
 * he wants each principal's sessions visible inside its tile "until we
 * stabilize clearing".
 */
import fs from 'fs';
import os from 'os';
import path from 'path';
import { TilePoller, sessionsLine } from '../src/tiles';

const NOW = new Date('2026-10-06T12:10:00Z'); // 08:10 Boston

describe('#4432 sessions inside the tile', () => {
  it('lists each open session: channel, start and last seen, Boston clock, date only when not today', () => {
    expect(sessionsLine([
      { channel: 'pane', startedAt: '2026-10-02T13:03:00Z', lastSeenAt: '2026-10-02T19:51:00Z' },
      { channel: 'agent', startedAt: '2026-10-06T12:46:00Z', lastSeenAt: '2026-10-06T12:50:00Z' },
    ], NOW)).toBe('2 sessions · pane 10-02 09:03 (seen 10-02 15:51) · agent 08:46 (seen 08:50)');
    expect(sessionsLine([{ channel: 'pane', startedAt: '2026-10-06T11:00:00Z' }], NOW)).toBe('1 session · pane 07:00 (seen ?)');
  });

  it('NEGATIVE PROOF: no open session means no line, never "0 sessions"', () => {
    expect(sessionsLine([], NOW)).toBe('');
    expect(sessionsLine(undefined, NOW)).toBe('');
  });

  it('one tile per principal: Wren with two sessions is one tile whose line names both', () => {
    const scanDir = fs.mkdtempSync(path.join(os.tmpdir(), 'tiles4432-'));
    const p = new TilePoller({
      scanDir, pulseFile: path.join(scanDir, 'pulse.json'), werkRunsDir: scanDir, chorusApi: 'http://127.0.0.1:9',
      readRoles: () => [{ role: 'wren', state: 'idle', sessions: [
        { channel: 'pane', startedAt: '2026-10-02T13:03:00Z', lastSeenAt: '2026-10-02T19:51:00Z' },
        { channel: 'pane', startedAt: '2026-10-06T12:46:00Z', lastSeenAt: '2026-10-06T12:50:00Z' },
      ] }],
    });
    const wren = p.getTiles().filter((t) => t.role === 'wren');
    expect(wren).toHaveLength(1);
    expect(wren[0].sessionsLine).toMatch(/^2 sessions · pane .* · pane /);
    fs.rmSync(scanDir, { recursive: true, force: true });
  });
});
