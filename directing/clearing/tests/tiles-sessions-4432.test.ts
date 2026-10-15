// @test-type: unit — injected role rows, mkdtemp scan dir, unreachable board API; brings its own world.
// @domain: messages — placed by chorus-crawl's domain rules (#4538)
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
  it('#4500 one short line: the count, and only the newest session in full (Jeff 10-12: four sessions ate his phone screen)', () => {
    expect(sessionsLine([
      { channel: 'pane', startedAt: '2026-10-02T13:03:00Z', lastSeenAt: '2026-10-02T19:51:00Z' },
      { channel: 'agent', startedAt: '2026-10-06T12:46:00Z', lastSeenAt: '2026-10-06T12:50:00Z' },
    ], NOW)).toBe('2 sessions · newest agent 08:46 (seen 08:50)');
    expect(sessionsLine([{ channel: 'pane', startedAt: '2026-10-06T11:00:00Z' }], NOW)).toBe('1 session · pane 07:00 (seen ?)');
  });

  it('#4500 NEGATIVE PROOF: four sessions never list four times, and the newest is the latest start, not the first row', () => {
    const line = sessionsLine([
      { channel: 'browser', startedAt: '2026-10-05T13:44:00Z', lastSeenAt: '2026-10-05T13:44:00Z' },
      { channel: 'browser', startedAt: '2026-10-06T10:17:00Z', lastSeenAt: '2026-10-06T10:17:00Z' },
      { channel: 'browser', startedAt: '2026-10-06T11:57:00Z', lastSeenAt: '2026-10-06T11:57:00Z' },
      { channel: 'browser', startedAt: '2026-10-06T12:01:00Z', lastSeenAt: '2026-10-06T12:09:00Z' },
    ], NOW);
    expect(line).toBe('4 sessions · newest browser 08:01 (seen 08:09)');
    expect(line.split('browser')).toHaveLength(2);
  });

  it('NEGATIVE PROOF: no open session means no line, never "0 sessions"', () => {
    expect(sessionsLine([], NOW)).toBe('');
    expect(sessionsLine(undefined, NOW)).toBe('');
  });

  it('one tile per principal: Wren with two sessions is one tile whose line counts both and names the newest', () => {
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
    expect(wren[0].sessionsLine).toMatch(/^2 sessions · newest pane (10-06 )?08:46 /);
    fs.rmSync(scanDir, { recursive: true, force: true });
  });
});
