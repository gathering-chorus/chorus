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
import { TilePoller } from '../src/tiles';

function tmpDirs() {
  const base = fs.mkdtempSync(path.join(os.tmpdir(), 'tiles3772-'));
  const scanDir = path.join(base, 'scan');
  const werkDir = path.join(base, 'werk-runs');
  fs.mkdirSync(scanDir); fs.mkdirSync(werkDir);
  return { base, scanDir, werkDir };
}

function poller(scanDir: string, werkDir: string) {
  return new TilePoller({
    scanDir,
    werkRunsDir: werkDir,
    pulseFile: path.join(scanDir, 'no-pulse.json'),
    chorusApi: 'http://127.0.0.1:1', // refused instantly; board facts not under test
  });
}

const wrenTile = (p: TilePoller) => p.getTiles().find((t) => t.role === 'wren')!;

describe('#3772 tiles — werk phase overrides the idle lie', () => {
  test('declared idle + running pipeline → building, with the card', () => {
    const { scanDir, werkDir } = tmpDirs();
    fs.writeFileSync(path.join(scanDir, 'wren-declared.json'),
      JSON.stringify({ state: 'idle', session_alive: true, ts: Math.floor(Date.now() / 1000) - 1140 }));
    fs.writeFileSync(path.join(werkDir, '3772.json'),
      JSON.stringify({ role: 'wren', card: 3772, phase: 'running' }));

    const t = wrenTile(poller(scanDir, werkDir));
    expect(t.state).toBe('building');
    expect(t.card).toBe('#3772');
    expect(t.sessionAlive).toBe(true);
  });

  test('presented run → presenting', () => {
    const { scanDir, werkDir } = tmpDirs();
    fs.writeFileSync(path.join(scanDir, 'wren-declared.json'), JSON.stringify({ state: 'idle' }));
    fs.writeFileSync(path.join(werkDir, '3761.json'),
      JSON.stringify({ role: 'wren', card: 3761, phase: 'presented' }));

    expect(wrenTile(poller(scanDir, werkDir)).state).toBe('presenting');
  });

  test('landed run does NOT override — landed is done, idle is true', () => {
    const { scanDir, werkDir } = tmpDirs();
    fs.writeFileSync(path.join(scanDir, 'wren-declared.json'), JSON.stringify({ state: 'idle' }));
    fs.writeFileSync(path.join(werkDir, '3768.json'),
      JSON.stringify({ role: 'wren', card: 3768, phase: 'landed' }));

    expect(wrenTile(poller(scanDir, werkDir)).state).toBe('idle');
  });

  test('declared blocked is NOT masked by a running pipeline', () => {
    const { scanDir, werkDir } = tmpDirs();
    fs.writeFileSync(path.join(scanDir, 'wren-declared.json'), JSON.stringify({ state: 'blocked' }));
    fs.writeFileSync(path.join(werkDir, '3772.json'),
      JSON.stringify({ role: 'wren', card: 3772, phase: 'running' }));

    expect(wrenTile(poller(scanDir, werkDir)).state).toBe('blocked');
  });

  test("another role's run never touches this tile", () => {
    const { scanDir, werkDir } = tmpDirs();
    fs.writeFileSync(path.join(scanDir, 'wren-declared.json'), JSON.stringify({ state: 'idle' }));
    fs.writeFileSync(path.join(werkDir, '3766.json'),
      JSON.stringify({ role: 'kade', card: 3766, phase: 'running' }));

    expect(wrenTile(poller(scanDir, werkDir)).state).toBe('idle');
  });

  test('missing werk-runs dir → declared state alone (no crash)', () => {
    const { scanDir } = tmpDirs();
    fs.writeFileSync(path.join(scanDir, 'wren-declared.json'), JSON.stringify({ state: 'idle' }));
    expect(wrenTile(poller(scanDir, '/nonexistent/nowhere')).state).toBe('idle');
  });
});
