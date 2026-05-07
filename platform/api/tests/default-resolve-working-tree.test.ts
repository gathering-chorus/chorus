/**
 * Test for #2779 latent L1 — `defaultResolveWorkingTree` cache never invalidated.
 *
 * Bug: cache keyed by role + never invalidated. First call locks the route for
 * daemon's lifetime. If settings.json gains/loses CHORUS_WERK_ENABLE, daemon
 * ignores until restart. Hit during 2026-05-07 session — daemon served wrong
 * route after settings.json drifted.
 *
 * Fix: drop the cache. Re-read settings.json on every call. File is small,
 * JSON parse is cheap, called per MCP request not per spine event.
 *
 * This test demonstrates the bug pre-fix (RED) and verifies the fix (GREEN):
 * - Create temp canonical with kade settings.json env={CHORUS_WERK_ENABLE: "0"}
 * - First call returns canonical (flag off)
 * - Mutate settings.json to env={CHORUS_WERK_ENABLE: "1"}
 * - Second call must return werk path (NOT cached canonical)
 */
import { defaultResolveWorkingTree } from '../src/mcp/server';
import * as fs from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';

describe('defaultResolveWorkingTree — settings.json drift', () => {
  let tmpRoot: string;

  beforeEach(() => {
    tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'chorus-resolve-test-'));
    // Create canonical-like layout: <tmpRoot>/canonical/roles/kade/.claude/settings.json
    const canonical = path.join(tmpRoot, 'canonical');
    const settingsDir = path.join(canonical, 'roles', 'kade', '.claude');
    fs.mkdirSync(settingsDir, { recursive: true });
    fs.writeFileSync(
      path.join(settingsDir, 'settings.json'),
      JSON.stringify({ env: { CHORUS_WERK_ENABLE: '0' } }),
    );
  });

  afterEach(() => {
    fs.rmSync(tmpRoot, { recursive: true, force: true });
  });

  test('flag flip in settings.json is observed on next call (no stale cache)', () => {
    const canonical = path.join(tmpRoot, 'canonical');
    const expectedWerk = path.join(tmpRoot, 'chorus-werk', 'kade');
    const resolve = defaultResolveWorkingTree(canonical);

    // First call: flag is "0" → routes to canonical
    expect(resolve('kade')).toBe(canonical);

    // Mutate settings.json: flip flag to "1"
    const settingsPath = path.join(canonical, 'roles', 'kade', '.claude', 'settings.json');
    fs.writeFileSync(
      settingsPath,
      JSON.stringify({ env: { CHORUS_WERK_ENABLE: '1' } }),
    );

    // Second call: flag is now "1" → MUST route to werk, not the cached canonical
    expect(resolve('kade')).toBe(expectedWerk);
  });

  test('flag flop (1 → 0) is also observed on next call', () => {
    const canonical = path.join(tmpRoot, 'canonical');
    const settingsPath = path.join(canonical, 'roles', 'kade', '.claude', 'settings.json');
    fs.writeFileSync(settingsPath, JSON.stringify({ env: { CHORUS_WERK_ENABLE: '1' } }));

    const expectedWerk = path.join(tmpRoot, 'chorus-werk', 'kade');
    const resolve = defaultResolveWorkingTree(canonical);

    expect(resolve('kade')).toBe(expectedWerk);

    fs.writeFileSync(settingsPath, JSON.stringify({ env: { CHORUS_WERK_ENABLE: '0' } }));

    expect(resolve('kade')).toBe(canonical);
  });

  test('missing settings.json returns canonical (flag-off default)', () => {
    const canonical = path.join(tmpRoot, 'canonical');
    fs.rmSync(path.join(canonical, 'roles', 'kade', '.claude', 'settings.json'));
    const resolve = defaultResolveWorkingTree(canonical);
    expect(resolve('kade')).toBe(canonical);
  });
});
