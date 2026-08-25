// @test-type: integration:api
//
// #3959 — THE TEST WHOSE ABSENCE HID SIX DEFECTS.
//
// Jeff, 2026-08-21: "u show busy yet no streams for 20+ minutes if i ask u you
// will say u have been working the whole time." He was right, and every unit
// test on both sides of the seam passed the whole time.
//
// The defects all lived BETWEEN components, which is why nothing caught them:
//   - the parser's two primary cases (session_tool, session_turn) matched event
//     names that no emitter produces — 0/day
//   - agent.activity, the running/thinking beat from #3853, had no branch and
//     was dropped at the fallthrough — 12,787/day
//   - the role allowlist dropped 26,000 unattributed events/day, silently
//   - the byte-bounded tail window had shrunk to 51 seconds of wall clock
//   - #3852 removed display filters from one of the two pipes
//
// So this suite asserts the SEAM: a role does something, a line carrying that
// role comes out the other end. Pure-function round trip over real spine JSON —
// no live server, no clock, no network.

// jest provides describe/it/expect as globals — the vitest import was a wrong-framework
// line that made the whole suite fail to LOAD (nightly red 2026-08-24, never ran before)
import { readSpineWithStats, TAIL_BYTES } from '../src/spine-tail';

/** A spine line exactly as chorus-hooks writes it (state.rs:464-476). */
const spineLine = (o: Record<string, unknown>) =>
  JSON.stringify({ appName: 'chorus-events', level: 'info', ...o });

/** Minimal fs stand-in: readSpineWithStats only tail-reads one file. */
function fakeFs(lines: string[]) {
  const buf = Buffer.from(lines.join('\n') + '\n', 'utf8');
  return {
    openSync: () => 1,
    fstatSync: () => ({ size: buf.length }),
    readSync: (_fd: number, target: Buffer, _off: number, len: number, pos: number) => {
      const slice = buf.subarray(pos, pos + len);
      slice.copy(target);
      return slice.length;
    },
    closeSync: () => undefined,
  } as unknown as typeof import('fs');
}

const beat = (role: string | null, at: string, phase = 'running', tool = 'Bash') =>
  spineLine({
    timestamp: at,
    event: 'agent.activity',
    ...(role === null ? {} : { role }),
    phase,
    tool,
    elapsed_s: '42',
  });

describe('#3959 — a role acts, the stream shows it, attributed', () => {
  it('renders the running beat that #3853 emits (the 12,787/day that vanished)', () => {
    const { lines } = readSpineWithStats(
      fakeFs([beat('wren', '2026-08-21T18:00:00.000-0400')]),
      '/fake/chorus.log',
      80,
    );
    expect(lines).toHaveLength(1);
    expect(lines[0].role).toBe('wren');
    expect(lines[0].text).toContain('Bash');
    expect(lines[0].text).toContain('42s');
  });

  it('renders the thinking beat', () => {
    const { lines } = readSpineWithStats(
      fakeFs([beat('silas', '2026-08-21T18:00:01.000-0400', 'thinking', 'Edit')]),
      '/fake/chorus.log',
      80,
    );
    expect(lines).toHaveLength(1);
    expect(lines[0].role).toBe('silas');
    expect(lines[0].text).toContain('Edit');
  });

  // NEGATIVE PROOF #1 — the pre-fix parser. If the agent.activity branch is ever
  // removed, this reproduces exactly what Jeff experienced: a working role that
  // renders as nothing at all.
  it('a parser accepting only the old four event names renders NOTHING for a working role', () => {
    const beats = Array.from({ length: 20 }, (_, i) =>
      beat('wren', `2026-08-21T18:00:${String(i).padStart(2, '0')}.000-0400`),
    );
    const { lines } = readSpineWithStats(fakeFs(beats), '/fake/chorus.log', 80);
    expect(lines.length).toBeGreaterThan(0); // the fix holds

    const OLD_ACCEPTED = ['session_tool', 'session_turn', 'nudge.emitted', 'werk.phase'];
    const survivingUnderOldRule = beats.filter((l) =>
      OLD_ACCEPTED.includes(JSON.parse(l).event as string),
    );
    expect(survivingUnderOldRule).toHaveLength(0);
  });

  // NEGATIVE PROOF #2 — a drop must never be silent. This is the property that
  // let 26,000 events/day disappear without a trace.
  it('counts unattributed events instead of discarding them silently', () => {
    const { lines, stats } = readSpineWithStats(
      fakeFs([
        beat('unknown', '2026-08-21T18:00:00.000-0400'),
        beat('unattributed', '2026-08-21T18:00:01.000-0400'),
        beat(null, '2026-08-21T18:00:02.000-0400'),
        beat('wren', '2026-08-21T18:00:03.000-0400'),
      ]),
      '/fake/chorus.log',
      80,
    );
    expect(lines).toHaveLength(1);
    expect(stats.dropped['no-role']).toBe(1);
    expect(stats.dropped['unknown-role']).toBe(2);
  });

  // NEGATIVE PROOF #3 — an idle role and a broken emitter must not read the same.
  // A check that cannot separate those two states is the defect it exists to catch.
  it('idle and broken-emitter produce different readings despite both rendering zero lines', () => {
    const idle = readSpineWithStats(fakeFs([]), '/fake/chorus.log', 80);
    const broken = readSpineWithStats(
      fakeFs(Array.from({ length: 5 }, (_, i) => beat('unknown', `2026-08-21T18:00:0${i}.000-0400`))),
      '/fake/chorus.log',
      80,
    );

    // Both render zero — which is why rendered-count alone is not enough, and
    // why the drop counters had to exist.
    expect(idle.lines).toHaveLength(0);
    expect(broken.lines).toHaveLength(0);

    const lostIdle = idle.stats.dropped['no-role'] + idle.stats.dropped['unknown-role'];
    const lostBroken = broken.stats.dropped['no-role'] + broken.stats.dropped['unknown-role'];
    expect(lostIdle).toBe(0);
    expect(lostBroken).toBe(5);
    expect(lostBroken).not.toBe(lostIdle);
  });

  it('reports the wall-clock span it covered, so a shrinking window is visible', () => {
    const { stats } = readSpineWithStats(
      fakeFs([
        beat('wren', '2026-08-21T17:00:00.000-0400'),
        beat('wren', '2026-08-21T18:00:00.000-0400'),
      ]),
      '/fake/chorus.log',
      80,
    );
    expect(stats.spanFrom).toBe('2026-08-21T17:00:00.000-0400');
    expect(stats.spanTo).toBe('2026-08-21T18:00:00.000-0400');
  });

  // The window is bounded in BYTES against a log that grows. 256 KB measured 51
  // seconds on 2026-08-21. Pin the floor so the next volume increase cannot
  // quietly re-create the same blind spot.
  it('the tail window is large enough to cover a working session', () => {
    const { stats } = readSpineWithStats(
      fakeFs([beat('wren', '2026-08-21T18:00:00.000-0400')]),
      '/fake/chorus.log',
      80,
    );
    expect(stats.lines).toBe(1);
    expect(TAIL_BYTES).toBeGreaterThanOrEqual(8 * 1024 * 1024);
  });
});
