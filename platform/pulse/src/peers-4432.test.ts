// @test-type: unit — fake fetch; no live services
// @domain: messages
/** #4432 — pulse learns its peers from the roles door, and refuses when it can't. */
import { peersFrom, fetchPeers, FetchLike } from './peers';
import { inferNudgeClass } from './store';

const DOOR = { data: [
  { name: 'abby-normal', roleKind: 'agent' }, { name: 'jeff', roleKind: 'human' },
  { name: 'kade', roleKind: 'agent' }, { name: 'nightly', roleKind: '' },
  { name: 'silas', roleKind: 'agent' }, { name: 'wren', roleKind: 'agent' },
] };
const answering = (body: unknown, ok = true, status = 200): FetchLike => () => Promise.resolve({ ok, status, json: () => Promise.resolve(body) });

test('peers are every agent and human role, Abby included', async () => {
  expect(await fetchPeers('http://door', answering(DOOR))).toEqual(['abby-normal', 'jeff', 'kade', 'silas', 'wren']);
});

test('a nudge from Abby is role-to-role, so it can owe a reply', () => {
  expect(inferNudgeClass('abby-normal', peersFrom(DOOR))).toBe('r2r');
  expect(inferNudgeClass('nightly', peersFrom(DOOR))).toBe('a2r');
});

test('NEGATIVE PROOF: a door that answers an error refuses, never the usual four', async () => {
  await expect(fetchPeers('http://door', answering({}, false, 502))).rejects.toThrow('HTTP 502');
});

test('NEGATIVE PROOF: a door with no data list or no peers refuses', () => {
  expect(() => peersFrom({ error: 'down' })).toThrow('no data list');
  expect(() => peersFrom({ data: [{ name: 'nightly', roleKind: '' }] })).toThrow('no agent or human role');
});
