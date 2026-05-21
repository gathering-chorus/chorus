// #3025 AC6 — the three Athena lookups read the v2 live graph
// (/api/athena/subdomains[/:id[/blast-radius]]), not the drifting v1 tree.json.
// ADR-031: one source of truth. Asserts the routes hit + shape mapping + the
// not-found path. The getter is injected so no live chorus-api is needed.

import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';
import { buildMcpServer, type ExecFileAsync } from '../src/server';
import { __setAthenaGetter } from '../src/athena-tree-stub';

const noopExec = (async () => ({ stdout: '', stderr: '' })) as unknown as ExecFileAsync;

async function call(name: string, args: Record<string, unknown>, paths: string[], reply: (p: string) => unknown) {
  __setAthenaGetter((p: string) => { paths.push(p); return reply(p); });
  const server = buildMcpServer(() => 'wren', { execFileAsync: noopExec, cardsPath: '/fake' });
  const [ct, st] = InMemoryTransport.createLinkedPair();
  const client = new Client({ name: 'athena-v2-test', version: '1.0' });
  await Promise.all([server.connect(st), client.connect(ct)]);
  try {
    const r = await client.callTool({ name, arguments: args }) as { content: Array<{ text: string }> };
    return JSON.parse(r.content[0].text);
  } finally {
    await client.close(); await server.close(); __setAthenaGetter(null);
  }
}

test('ownership_lookup reads v2 subdomains/:id and maps owner', async () => {
  const paths: string[] = [];
  const out = await call('chorus_ownership_lookup', { iri: 'chorus:cards-service' }, paths,
    () => ({ data: { id: 'cards-service', label: 'Cards (Service)', owner: 'Wren', step: 'Directing' } }));
  assert.ok(paths.includes('/api/athena/subdomains/cards-service'), `hit v2 route, got ${paths}`);
  assert.equal(out.owner, 'Wren');
  assert.equal(out.id, 'cards-service');
});

test('blast_radius reads v2 subdomains/:id/blast-radius and maps consumers', async () => {
  const paths: string[] = [];
  const out = await call('chorus_blast_radius', { iri: 'chorus:cards-service' }, paths,
    () => ({ data: { subdomain: 'cards-service', consumers: [{ uri: 'x', label: 'Loom' }] } }));
  assert.ok(paths.includes('/api/athena/subdomains/cards-service/blast-radius'), `hit v2 route, got ${paths}`);
  assert.equal(out.consumers.length, 1);
});

test('tree_get reads the v2 subdomains list', async () => {
  const paths: string[] = [];
  const out = await call('chorus_tree_get', {}, paths,
    () => ({ data: [{ id: 'cards-service', owner: 'Wren' }] }));
  assert.ok(paths.includes('/api/athena/subdomains'), `hit v2 list, got ${paths}`);
  assert.equal(out[0].id, 'cards-service');
});

test('unknown id maps to not-found (no v1 confidently-wrong answer)', async () => {
  const paths: string[] = [];
  const out = await call('chorus_ownership_lookup', { iri: 'chorus:nope' }, paths,
    () => ({ data: null }));
  assert.equal(out.ok, false);
  assert.equal(out.reason, 'not-found');
});
