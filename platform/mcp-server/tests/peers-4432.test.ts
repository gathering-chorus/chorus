// @test-type: unit — fake fetch; no live services
// @domain: messages
/** #4432 — the MCP server learns roles and nudge peers from the roles door. */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { roleSetsFrom, fetchRoleSets, resetRoleSetsCache } from '../src/peers';

const DOOR = { data: [
  { name: 'abby-normal', roleKind: 'agent' }, { name: 'jeff', roleKind: 'human' },
  { name: 'kade', roleKind: 'agent' }, { name: 'nightly', roleKind: '' },
  { name: 'silas', roleKind: 'agent' }, { name: 'wren', roleKind: 'agent' },
] };
const answering = (body: unknown, ok = true, status = 200) =>
  async () => ({ ok, status, json: async () => body });

test('peers are agents + humans and Abby is one; agents exclude Jeff', () => {
  const s = roleSetsFrom(DOOR);
  assert.deepEqual(s.peers, ['abby-normal', 'jeff', 'kade', 'silas', 'wren']);
  assert.deepEqual(s.agents, ['abby-normal', 'kade', 'silas', 'wren']);
});

test('NEGATIVE PROOF: an erroring door throws, never the old four', async () => {
  resetRoleSetsCache();
  await assert.rejects(fetchRoleSets('http://door', answering({}, false, 502), 1), /HTTP 502/);
});

test('NEGATIVE PROOF: no data list, or no agent role, throws', () => {
  assert.throws(() => roleSetsFrom({ error: 'down' }), /no data list/);
  assert.throws(() => roleSetsFrom({ data: [{ name: 'jeff', roleKind: 'human' }] }), /no agent role/);
});

test('an answer is reused for 30s, then re-read; a failed re-read throws', async () => {
  resetRoleSetsCache();
  await fetchRoleSets('http://door', answering(DOOR), 1000);
  const again = await fetchRoleSets('http://door', answering({}, false, 502), 20_000);
  assert.ok(again.peers.includes('abby-normal'));
  await assert.rejects(fetchRoleSets('http://door', answering({}, false, 502), 40_000), /HTTP 502/);
});
