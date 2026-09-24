import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { pinProfileBinding, type ProfileBinding } from '../src/stdio-session-binding';

test('profile bridge pins its conversation, verifies each call, and never follows a replacement session', async () => {
  let current: ProfileBinding = { session_id: 'one', principal: 'https://identity.test/wren', role: 'wren' };
  let reads = 0;
  const resolve = pinProfileBinding('opencode-wren', 'wren', async (profile, role) => {
    assert.equal(profile, 'opencode-wren'); assert.equal(role, 'wren'); reads++; return current;
  });
  assert.equal((await resolve()).session_id, 'one');
  assert.equal((await resolve()).session_id, 'one');
  current = { ...current, session_id: 'replacement' };
  await assert.rejects(resolve(), /changed-restart-bridge/);
  assert.equal(reads, 3);
});

test('profile bridge refuses role, explicit session, principal changes, and unavailable bindings', async () => {
  const current = { session_id: 'one', principal: 'https://identity.test/wren', role: 'wren' };
  await assert.rejects(pinProfileBinding('profile', 'silas', async () => current)(), /role-or-session-mismatch/);
  await assert.rejects(pinProfileBinding('profile', 'wren', async () => current, 'other')(), /role-or-session-mismatch/);
  const resolve = pinProfileBinding('profile', 'wren', async () => current);
  await resolve(); current.principal = 'https://identity.test/other';
  await assert.rejects(resolve(), /changed-restart-bridge/);
  await assert.rejects(pinProfileBinding('profile', 'wren', async () => { throw Error('ambiguous'); })(), /ambiguous/);
});
