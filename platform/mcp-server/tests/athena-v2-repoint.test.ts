// @test-type: unit — stubbed dependencies; no live service.
// #3025 AC6 (CORRECTED 2026-05-22) — the three Athena lookups read the v2 JSON
// tree, served by chorus-api at /api/athena/tree and
// /api/athena/blast-radius/:iri (all answered from the live graph, #4235). They must NOT read the AS-IS Fuseki per-domain routes the design
// says v2 replaces, which drops products that live only in the
// JSON tree (e.g. The Clearing). Asserts route hit + shape passthrough + not-found.
// The getter is injected so no live chorus-api is needed; buildMcpServer is the
// production entry under test in each case.

import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';
import { buildMcpServer, type ExecFileAsync } from '../src/server';
import { __setAthenaGetter } from '../src/athena-tree-stub';

const noopExec = (async () => ({ stdout: '', stderr: '' })) as unknown as ExecFileAsync;

test('#4508 — ownership has one answer: chorus_ownership_lookup is not a tool any more', async () => {
  const server = buildMcpServer(() => 'wren', { execFileAsync: noopExec, cardsPath: '/fake' });
  const [ct, st] = InMemoryTransport.createLinkedPair();
  const client = new Client({ name: 'athena-v2-test', version: '1.0' });
  await Promise.all([server.connect(st), client.connect(ct)]);
  try {
    const names = (await client.listTools()).tools.map((t) => t.name);
    assert.ok(names.includes('chorus_tree_get'), 'control: the tree tool is still listed');
    assert.ok(!names.includes('chorus_ownership_lookup'), 'the model door answers ownership; this copy must stay deleted');
  } finally {
    await client.close(); await server.close();
  }
});

test('blast_radius reads the blast-radius route and passes consumers through', async () => {
  const paths: string[] = [];
  __setAthenaGetter((p: string) => {
    paths.push(p);
    return { iri: 'chorus:cards', consumers: ['chorus:clearing'], dependents: [], hosts: [] };
  });
  const server = buildMcpServer(() => 'wren', { execFileAsync: noopExec, cardsPath: '/fake' });
  const [ct, st] = InMemoryTransport.createLinkedPair();
  const client = new Client({ name: 'athena-v2-test', version: '1.0' });
  await Promise.all([server.connect(st), client.connect(ct)]);
  try {
    const r = await client.callTool({ name: 'chorus_blast_radius', arguments: { iri: 'chorus:cards' } }) as { content: Array<{ text: string }> };
    const out = JSON.parse(r.content[0].text);
    assert.ok(paths.includes('/api/athena/blast-radius/chorus%3Acards'), `hit v2 tree blast route, got ${paths}`);
    assert.equal(out.consumers.length, 1);
    assert.equal(out.consumers[0], 'chorus:clearing');
  } finally {
    await client.close(); await server.close(); __setAthenaGetter(null);
  }
});

test('tree_get reads the full v2 JSON tree (products incl The Clearing)', async () => {
  const paths: string[] = [];
  __setAthenaGetter((p: string) => {
    paths.push(p);
    return { schemaVersion: 'athena-move-0/2026-05-16', products: [{ label: 'The Clearing' }], domains: [] };
  });
  const server = buildMcpServer(() => 'wren', { execFileAsync: noopExec, cardsPath: '/fake' });
  const [ct, st] = InMemoryTransport.createLinkedPair();
  const client = new Client({ name: 'athena-v2-test', version: '1.0' });
  await Promise.all([server.connect(st), client.connect(ct)]);
  try {
    const r = await client.callTool({ name: 'chorus_tree_get', arguments: {} }) as { content: Array<{ text: string }> };
    const out = JSON.parse(r.content[0].text);
    assert.ok(paths.includes('/api/athena/tree'), `hit v2 JSON tree, got ${paths}`);
    assert.equal(out.products[0].label, 'The Clearing');
  } finally {
    await client.close(); await server.close(); __setAthenaGetter(null);
  }
});
