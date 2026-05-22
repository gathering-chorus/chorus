// #3025 AC6 (CORRECTED 2026-05-22) — the three Athena lookups read the v2 JSON
// tree, served by chorus-api at /api/athena/tree, /api/athena/ownership/:iri,
// /api/athena/blast-radius/:iri (all backed by data/athena/tree.json, Move 0 of
// Athena v2). They must NOT read /api/athena/subdomains — the AS-IS Fuseki
// surface the design says v2 replaces, which drops products that live only in the
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

test('ownership_lookup reads the v2 tree.json route and passes owner + product through', async () => {
  const paths: string[] = [];
  __setAthenaGetter((p: string) => {
    paths.push(p);
    return { iri: 'chorus:cards', kind: 'domain', owner: 'chorus:role-wren', product: 'chorus:clearing', domain: 'chorus:cards' };
  });
  const server = buildMcpServer(() => 'wren', { execFileAsync: noopExec, cardsPath: '/fake' });
  const [ct, st] = InMemoryTransport.createLinkedPair();
  const client = new Client({ name: 'athena-v2-test', version: '1.0' });
  await Promise.all([server.connect(st), client.connect(ct)]);
  try {
    const r = await client.callTool({ name: 'chorus_ownership_lookup', arguments: { iri: 'chorus:cards' } }) as { content: Array<{ text: string }> };
    const out = JSON.parse(r.content[0].text);
    assert.ok(paths.includes('/api/athena/ownership/chorus%3Acards'), `hit v2 tree ownership route, got ${paths}`);
    assert.equal(out.owner, 'chorus:role-wren');
    assert.equal(out.product, 'chorus:clearing'); // The Clearing survives — the answer the v1 regression dropped.
  } finally {
    await client.close(); await server.close(); __setAthenaGetter(null);
  }
});

test('blast_radius reads the v2 tree.json route and passes consumers through', async () => {
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

test('unknown iri maps to not-found (no confidently-wrong answer)', async () => {
  const paths: string[] = [];
  __setAthenaGetter((p: string) => { paths.push(p); return {}; });
  const server = buildMcpServer(() => 'wren', { execFileAsync: noopExec, cardsPath: '/fake' });
  const [ct, st] = InMemoryTransport.createLinkedPair();
  const client = new Client({ name: 'athena-v2-test', version: '1.0' });
  await Promise.all([server.connect(st), client.connect(ct)]);
  try {
    const r = await client.callTool({ name: 'chorus_ownership_lookup', arguments: { iri: 'chorus:nope' } }) as { content: Array<{ text: string }> };
    const out = JSON.parse(r.content[0].text);
    assert.ok(paths.includes('/api/athena/ownership/chorus%3Anope'), `hit v2 route, got ${paths}`);
    assert.equal(out.ok, false);
    assert.equal(out.reason, 'not-found');
  } finally {
    await client.close(); await server.close(); __setAthenaGetter(null);
  }
});
