// @test-type: unit — signal:integration is the temp files. Reading a file IS the
// behaviour under test (a session token on disk), so the fixture is the unit's
// input, not a live dependency: no service, no network, no shared state.
// #4202 — a verb's identity comes from the session, never from the typed role.
//
// Before this, executeWerkVerb passed DEPLOY_ROLE and CHORUS_ROLE straight from
// the tool's `role` argument and attached no token. Two consequences, both
// measured on 2026-09-17:
//
//   1. athena-model refuses env-trust (#3687, fail closed), so every governed
//      write through MCP came back "identity-token-required" — the governed
//      path was simply dead.
//   2. the argument is whatever the caller typed, so nothing about a write
//      could prove which role made it.
//
// The fix reads the session's token file. The tests below exist to separate
// "a real session" from everything else, because a helper that substituted a
// plausible token on a missing file would rebuild the original defect.

import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { mkdtempSync, writeFileSync, chmodSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { sessionTokenEnv } from '../src/server';

const dir = mkdtempSync(join(tmpdir(), '4202-session-'));

// Shape only — three dot-separated segments. Not a live credential.
const JWT = 'aGVhZGVy.cGF5bG9hZA.c2ln';

test('a session token file rides along as the identity token', () => {
  const f = join(dir, 'good.jwt');
  writeFileSync(f, JWT + '\n');
  assert.deepEqual(sessionTokenEnv(f), { CHORUS_IDENTITY_TOKEN: JWT });
});

test('NEGATIVE PROOF — no session file means no token, not a substitute', () => {
  assert.deepEqual(sessionTokenEnv(undefined), {});
  assert.deepEqual(sessionTokenEnv(''), {});
});

test('NEGATIVE PROOF — a path that does not exist yields nothing', () => {
  assert.deepEqual(sessionTokenEnv(join(dir, 'missing.jwt')), {});
});

test('NEGATIVE PROOF — a file that is not a JWT yields nothing', () => {
  const f = join(dir, 'junk.jwt');
  writeFileSync(f, 'not a token');
  assert.deepEqual(sessionTokenEnv(f), {});

  const half = join(dir, 'half.jwt');
  writeFileSync(half, 'header.payload');
  assert.deepEqual(sessionTokenEnv(half), {});
});

test('NEGATIVE PROOF — an unreadable file yields nothing rather than throwing', () => {
  const f = join(dir, 'locked.jwt');
  writeFileSync(f, JWT);
  chmodSync(f, 0o000);
  // Running as root would read it regardless, and then this proves nothing.
  if (process.getuid && process.getuid() === 0) return;
  assert.deepEqual(sessionTokenEnv(f), {});
});
