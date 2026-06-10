// #3173 — acp's promote-werk-bin must read the role's LIVE deploy slot
// ($WERK_<ROLE>_BIN = chorus-werk/<role>-bin/), exactly where werk-deploy /
// werk-deploy --target werk installs and the verb wrapper resolves. The pre-fix
// code read `repoRoot/.werk-bin` — a RETIRED slot (chorus-env-setup.sh:104, it broke
// with >1 card open) that no longer exists at deploy time. So existsSync was false,
// the promote silently no-op'd, and canonical ~/.chorus/bin/<verb>-bin stayed stale
// while acp returned success: merged≠live for the whole verb suite (verified 2026-06-01,
// every canonical verb binary days/weeks stale). These pin the one correct slot.
import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { resolveWerkBinDir } from '../src/server';

test('#3173 derives the role-bin slot from CHORUS_WERK_BASE — NOT repoRoot/.werk-bin (the bug)', () => {
  const dir = resolveWerkBinDir('kade', { CHORUS_WERK_BASE: '/x/chorus-werk' }, '/x/chorus');
  assert.equal(dir, '/x/chorus-werk/kade-bin');
  assert.notEqual(dir, '/x/chorus/.werk-bin');
});

test('#3173 prefers the explicit WERK_<ROLE>_BIN env — the same var deploy + wrapper use (one convention)', () => {
  const dir = resolveWerkBinDir('silas', { WERK_SILAS_BIN: '/custom/silas-bin', CHORUS_WERK_BASE: '/x/chorus-werk' }, '/x/chorus');
  assert.equal(dir, '/custom/silas-bin');
});

test('#3173 falls back to repoRoot-sibling chorus-werk/<role>-bin when CHORUS_WERK_BASE unset', () => {
  const dir = resolveWerkBinDir('wren', {}, '/x/chorus');
  assert.equal(dir, '/x/chorus-werk/wren-bin');
});
