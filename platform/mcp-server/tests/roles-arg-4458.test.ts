// @test-type: unit — pure role checks with a stubbed roles door; no service. node:test/tsx (the mcp-server runner).
// #4458 — Abby (abby-normal) owned #4456 and could not pull it: every werk
// verb's schema said kade | wren | silas. A role argument is now any role the
// roles door lists, checked before the tool runs.

import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { AgentRole, unknownRoleError, refuseUnknownRoles } from '../src/roles-arg';

const SETS = { agents: ['abby-normal', 'kade', 'silas', 'wren'], peers: ['abby-normal', 'jeff', 'kade', 'silas', 'wren'] };

test('abby-normal passes the schema and the door check for a werk verb', async () => {
  assert.equal(AgentRole.safeParse('abby-normal').success, true);
  assert.equal(unknownRoleError('werk-pull', { role: 'abby-normal', card_id: 4456 }, SETS), null);
  await refuseUnknownRoles('werk-pull', { role: 'abby-normal' }, async () => SETS);
});

test('NEGATIVE PROOF: a role the door does not list is refused by name', async () => {
  assert.equal(unknownRoleError('werk-pull', { role: 'bob' }, SETS), "Unknown role 'bob' — the roles door lists abby-normal | kade | silas | wren");
  await assert.rejects(refuseUnknownRoles('chorus_werk', { role: 'bob' }, async () => SETS), /Unknown role 'bob'/);
});

test('a human may own a card, but only an agent may be a calling role', () => {
  assert.equal(unknownRoleError('chorus_card_add_jeff', { owner: 'jeff' }, SETS), null);
  assert.match(String(unknownRoleError('werk-pull', { role: 'jeff' }, SETS)), /Unknown role 'jeff'/);
});

test('the schema refuses a value that is not a role name', () => {
  assert.equal(AgentRole.safeParse('../etc').success, false);
  assert.equal(AgentRole.safeParse('Kade').success, false);
});

test('a log filter role (system) is not a caller role and is not checked', () => {
  assert.equal(unknownRoleError('chorus_logs_recent_errors', { role: 'system' }, SETS), null);
});

test('a call with no role argument never reads the door', async () => {
  let reads = 0;
  await refuseUnknownRoles('chorus_migration_readout', {}, async () => { reads += 1; return SETS; });
  assert.equal(reads, 0);
});
