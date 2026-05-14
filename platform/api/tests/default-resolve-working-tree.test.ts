/**
 * Test for #2913 — `defaultResolveWorkingTree` ephemeral-worktree resolution.
 *
 * The #2750 CHORUS_WERK_ENABLE flag-router is gone. Under the ephemeral model
 * (chorus-werk/<role>-<card>/) the resolver globs chorus-werk/<role>-* :
 *   - exactly one match  → that is the role's active card werk
 *   - zero matches       → no card in flight; fall back to canonical
 *   - multiple matches   → ambiguous; fall back to canonical (the >1-card case
 *                          needs an explicit card_id — #2920)
 *
 * No cache (the #2779 lesson): the set of werk dirs changes within a session
 * as cards are pulled and acp'd; resolution must re-glob on every call.
 */
import { defaultResolveWorkingTree } from '../src/mcp/server';
import * as fs from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';

describe('defaultResolveWorkingTree — ephemeral worktree resolution (#2913)', () => {
  let tmpRoot: string;
  let canonical: string;
  let werkBase: string;

  beforeEach(() => {
    tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'chorus-resolve-test-'));
    canonical = path.join(tmpRoot, 'canonical');
    werkBase = path.join(tmpRoot, 'chorus-werk');
    fs.mkdirSync(canonical, { recursive: true });
  });

  afterEach(() => {
    fs.rmSync(tmpRoot, { recursive: true, force: true });
  });

  const mkWerk = (name: string): string => {
    const p = path.join(werkBase, name);
    fs.mkdirSync(p, { recursive: true });
    return p;
  };

  test('zero werk dirs → canonical (no card in flight)', () => {
    const resolve = defaultResolveWorkingTree(canonical);
    expect(resolve('kade')).toBe(canonical);
  });

  test('werk base missing entirely → canonical', () => {
    // werkBase never created
    const resolve = defaultResolveWorkingTree(canonical);
    expect(resolve('kade')).toBe(canonical);
  });

  test('exactly one <role>-* werk → that werk', () => {
    const werk = mkWerk('kade-2913');
    const resolve = defaultResolveWorkingTree(canonical);
    expect(resolve('kade')).toBe(werk);
  });

  test('multiple <role>-* werks → canonical (ambiguous, do not guess)', () => {
    mkWerk('kade-2913');
    mkWerk('kade-2914');
    const resolve = defaultResolveWorkingTree(canonical);
    expect(resolve('kade')).toBe(canonical);
  });

  test('only other roles\' werks present → canonical (no <role>-* match)', () => {
    mkWerk('wren-3000');
    mkWerk('silas-3001');
    const resolve = defaultResolveWorkingTree(canonical);
    expect(resolve('kade')).toBe(canonical);
  });

  test('one own werk alongside other roles\' werks → own werk', () => {
    mkWerk('wren-3000');
    const werk = mkWerk('kade-2913');
    mkWerk('silas-3001');
    const resolve = defaultResolveWorkingTree(canonical);
    expect(resolve('kade')).toBe(werk);
  });

  test('no cache — a werk appearing is observed on the next call', () => {
    const resolve = defaultResolveWorkingTree(canonical);
    expect(resolve('kade')).toBe(canonical); // none yet
    const werk = mkWerk('kade-2913');
    expect(resolve('kade')).toBe(werk); // appeared — observed, not stale
  });

  test('no cache — a werk disappearing is observed on the next call', () => {
    const werk = mkWerk('kade-2913');
    const resolve = defaultResolveWorkingTree(canonical);
    expect(resolve('kade')).toBe(werk);
    fs.rmSync(werk, { recursive: true, force: true });
    expect(resolve('kade')).toBe(canonical); // gone — observed
  });

  test('a plain file named <role>-x in werk base is ignored (dirs only)', () => {
    fs.mkdirSync(werkBase, { recursive: true });
    fs.writeFileSync(path.join(werkBase, 'kade-notadir'), 'x');
    const resolve = defaultResolveWorkingTree(canonical);
    expect(resolve('kade')).toBe(canonical);
  });
});
