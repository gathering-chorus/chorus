// @test-type: unit — mkdtemp fixture dirs for scan + werk-runs, unreachable board API; brings its own world.
/**
 * #3772 — tiles stop lying about idle.
 *
 * Jeff's 10:17 screenshot: Wren "idle 19m" while her pipeline ran. Presence now
 * derives from the werk run pin as well as the declared state: a role with a
 * run in phase running/presented never shows bare idle. Declared non-idle
 * states (blocked, observing) are not overridden — only the idle lie.
 */
import fs from 'fs';
import os from 'os';
import path from 'path';
import { TilePoller, setPidStartTimesForTest } from '../src/tiles';

function tmpDirs() {
  const base = fs.mkdtempSync(path.join(os.tmpdir(), 'tiles3772-'));
  const scanDir = path.join(base, 'scan');
  const werkDir = path.join(base, 'werk-runs');
  fs.mkdirSync(scanDir); fs.mkdirSync(werkDir);
  return { base, scanDir, werkDir };
}

// #4028 — role STATE comes from the derived endpoint, not a declared file. The
// helper takes wren's derived row so each case states what the streams said.
function poller(scanDir: string, werkDir: string, wren: { state: string; stale?: boolean } = { state: 'idle', stale: true }) {
  return new TilePoller({
    scanDir,
    werkRunsDir: werkDir,
    pulseFile: path.join(scanDir, 'no-pulse.json'),
    chorusApi: 'http://127.0.0.1:1', // refused instantly; board facts not under test
    readRoles: () => [{ role: 'wren', state: wren.state, stale: wren.stale ?? true, lastActivity: null }],
  });
}

const wrenTile = (p: TilePoller) => p.getTiles().find((t) => t.role === 'wren')!;

afterEach(() => setPidStartTimesForTest(null));

describe('#3772 tiles — werk phase overrides the idle lie', () => {
  test('declared idle + running pipeline → building, with the card', () => {
    const { scanDir, werkDir } = tmpDirs();
    fs.writeFileSync(path.join(werkDir, '3772.json'),
      JSON.stringify({ role: 'wren', card: 3772, phase: 'running', pid: process.pid, startedAt: new Date().toISOString() }));
    setPidStartTimesForTest(new Map([[process.pid, Date.now()]]));

    const t = wrenTile(poller(scanDir, werkDir));
    expect(t.state).toBe('building');
    expect(t.card).toBe('#3772');
    expect(t.sessionAlive).toBe(true);
  });

  test('presented run → presenting', () => {
    const { scanDir, werkDir } = tmpDirs();
    fs.writeFileSync(path.join(werkDir, '3761.json'),
      JSON.stringify({ role: 'wren', card: 3761, phase: 'presented', presentedAt: new Date().toISOString() }));

    expect(wrenTile(poller(scanDir, werkDir)).state).toBe('presenting');
  });

  test('landed run does NOT override — landed is done, idle is true', () => {
    const { scanDir, werkDir } = tmpDirs();
    fs.writeFileSync(path.join(werkDir, '3768.json'),
      JSON.stringify({ role: 'wren', card: 3768, phase: 'landed' }));

    expect(wrenTile(poller(scanDir, werkDir)).state).toBe('idle');
  });

  test('declared blocked is NOT masked by a running pipeline', () => {
    const { scanDir, werkDir } = tmpDirs();
    fs.writeFileSync(path.join(werkDir, '3772.json'),
      JSON.stringify({ role: 'wren', card: 3772, phase: 'running', pid: process.pid, startedAt: new Date().toISOString() }));
    setPidStartTimesForTest(new Map([[process.pid, Date.now()]]));

    expect(wrenTile(poller(scanDir, werkDir, { state: 'blocked', stale: false })).state).toBe('blocked');
  });

  test("another role's run never touches this tile", () => {
    const { scanDir, werkDir } = tmpDirs();
    fs.writeFileSync(path.join(werkDir, '3766.json'),
      JSON.stringify({ role: 'kade', card: 3766, phase: 'running', pid: process.pid, startedAt: new Date().toISOString() }));
    setPidStartTimesForTest(new Map([[process.pid, Date.now()]]));

    expect(wrenTile(poller(scanDir, werkDir)).state).toBe('idle');
  });

  // ---- #3780: stale-pin liveness — the NEGATIVE PROOF ----
  // Red against #3772's shipped behavior (phase trusted alone), green after.

  test('#3780 running pin with a DEAD pid is ignored — idle stays idle', () => {
    const { scanDir, werkDir } = tmpDirs();
    fs.writeFileSync(path.join(werkDir, '3432.json'),
      JSON.stringify({ role: 'wren', card: 3432, phase: 'running', pid: 999901, startedAt: '2026-06-23T16:14:06Z' }));
    setPidStartTimesForTest(new Map()); // pid not in the live map = dead

    expect(wrenTile(poller(scanDir, werkDir)).state).toBe('idle');
  });

  test('#3781 NEGATIVE PROOF: LIVE pid whose start time mismatches the pin (pid reuse) is ignored', () => {
    // The exact live failure: pin 3467 from June 17 carried pid 42578, alive
    // on Aug 6 as a Chrome renderer started July 23. Red against #3780's
    // existence-only check; green with identity matching.
    const { scanDir, werkDir } = tmpDirs();
    fs.writeFileSync(path.join(werkDir, '3467.json'),
      JSON.stringify({ role: 'wren', card: 3467, phase: 'running', pid: 42578, startedAt: '2026-06-17T22:20:22Z' }));
    setPidStartTimesForTest(new Map([[42578, Date.parse('2026-07-23T11:02:00Z')]])); // alive — but July, not June

    expect(wrenTile(poller(scanDir, werkDir)).state).toBe('idle');
  });

  test('#3781 running pin with no startedAt is ignored (identity unverifiable = fail closed)', () => {
    const { scanDir, werkDir } = tmpDirs();
    fs.writeFileSync(path.join(werkDir, '3499.json'),
      JSON.stringify({ role: 'wren', card: 3499, phase: 'running', pid: process.pid }));
    setPidStartTimesForTest(new Map([[process.pid, Date.now()]]));

    expect(wrenTile(poller(scanDir, werkDir)).state).toBe('idle');
  });

  test('#3780 running pin with NO pid at all is ignored', () => {
    const { scanDir, werkDir } = tmpDirs();
    fs.writeFileSync(path.join(werkDir, '3432.json'),
      JSON.stringify({ role: 'wren', card: 3432, phase: 'running' }));

    expect(wrenTile(poller(scanDir, werkDir)).state).toBe('idle');
  });

  test('#3780 presented pin older than 24h is ignored', () => {
    const { scanDir, werkDir } = tmpDirs();
    fs.writeFileSync(path.join(werkDir, '3453.json'),
      JSON.stringify({ role: 'wren', card: 3453, phase: 'presented', presentedAt: '2026-06-16T15:47:19Z' }));

    expect(wrenTile(poller(scanDir, werkDir)).state).toBe('idle');
  });

  test('missing werk-runs dir → declared state alone (no crash)', () => {
    const { scanDir } = tmpDirs();
    expect(wrenTile(poller(scanDir, '/nonexistent/nowhere')).state).toBe('idle');
  });
});
