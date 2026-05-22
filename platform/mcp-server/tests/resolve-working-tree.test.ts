// #3038 — defaultResolveWorkingTree must select the card werk only, never the
// per-role `<role>-bin` binary slot (created by chorus-env-setup.sh). The bin
// slot shares the `chorus-werk/<role>-*` namespace; counting it makes the
// resolver return the bin slot (1 match at pull time → branch-fail) or canonical
// (2 matches at commit time → "On branch main"). These tests pin the correct
// selection: card werk = `<role>-<digits>`, nothing else.
import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import * as fs from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';
import { defaultResolveWorkingTree } from '../src/server';

function setup(dirs: string[]): { canonical: string; werkBase: string; cleanup: () => void } {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'resolve-wt-'));
  const canonical = path.join(tmp, 'chorus');
  const werkBase = path.join(tmp, 'chorus-werk');
  fs.mkdirSync(canonical, { recursive: true });
  fs.mkdirSync(werkBase, { recursive: true });
  for (const d of dirs) fs.mkdirSync(path.join(werkBase, d), { recursive: true });
  return { canonical, werkBase, cleanup: () => fs.rmSync(tmp, { recursive: true, force: true }) };
}

test('#3038 returns the card werk when a -bin slot also exists (bug: returned canonical)', () => {
  const { canonical, werkBase, cleanup } = setup(['kade-bin', 'kade-3038']);
  try { assert.equal(defaultResolveWorkingTree(canonical)('kade'), path.join(werkBase, 'kade-3038')); }
  finally { cleanup(); }
});

test('#3038 returns canonical when only the -bin slot exists (bug: returned the bin slot)', () => {
  const { canonical, cleanup } = setup(['kade-bin']);
  try { assert.equal(defaultResolveWorkingTree(canonical)('kade'), canonical); }
  finally { cleanup(); }
});

test('#3038 returns the card werk when only it exists', () => {
  const { canonical, werkBase, cleanup } = setup(['kade-3038']);
  try { assert.equal(defaultResolveWorkingTree(canonical)('kade'), path.join(werkBase, 'kade-3038')); }
  finally { cleanup(); }
});

test('#3038 returns canonical when no werk exists', () => {
  const { canonical, cleanup } = setup([]);
  try { assert.equal(defaultResolveWorkingTree(canonical)('kade'), canonical); }
  finally { cleanup(); }
});

test('#3038 does not leak across roles — wren-bin + wren-3025 ignored when resolving kade', () => {
  const { canonical, werkBase, cleanup } = setup(['wren-bin', 'wren-3025', 'kade-3038']);
  try { assert.equal(defaultResolveWorkingTree(canonical)('kade'), path.join(werkBase, 'kade-3038')); }
  finally { cleanup(); }
});
