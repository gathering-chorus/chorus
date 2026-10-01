// @test-type: unit — an in-memory MCP client against a stub fetch; no live services
// #4353 — chorus_principles_create writes the generated route with the fields
// the Principle shape requires, never the retired SubDomain route.
import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';
import { buildMcpServer, type FetchImpl } from '../src/server';

async function withClient(fetchImpl: FetchImpl, fn: (c: Client) => Promise<void>): Promise<void> {
  const server = buildMcpServer(() => 'wren', { fetchImpl });
  const [ct, st] = InMemoryTransport.createLinkedPair();
  const client = new Client({ name: 'principles-create-4353', version: '1.0' });
  await Promise.all([server.connect(st), client.connect(ct)]);
  try { await fn(client); } finally { await client.close(); await server.close(); }
}

test('create POSTs the shape\'s fields to /v1/principles/principles', async () => {
  const calls: Array<{ url: string; method?: string; body?: Record<string, string> }> = [];
  const fetchImpl = (async (url: string, init?: { method?: string; body?: string }) => {
    calls.push({ url, method: init?.method, body: init?.body ? JSON.parse(init.body) : undefined });
    return { ok: true, status: 201, json: async () => ({ data: { name: 'ship-small' } }) };
  }) as unknown as FetchImpl;
  await withClient(fetchImpl, async (c) => {
    const r = (await c.callTool({ name: 'chorus_principles_create', arguments: {
      label: 'Ship Small', comment: 'c', techReading: 't', jeffReading: 'j', principleKind: 'xp', source: 's', rhymesWith: 'hemenway-observe',
    } })) as { content: Array<{ text: string }> };
    assert.match(r.content[0].text, /ship-small/);
  });
  assert.equal(calls.length, 1);
  assert.ok(calls[0].url.endsWith('/v1/principles/principles'), calls[0].url);
  assert.equal(calls[0].method, 'POST');
  assert.deepEqual(calls[0].body, {
    name: 'ship-small', label: 'Ship Small', comment: 'c', techReading: 't', jeffReading: 'j', principleKind: 'xp', source: 's', rhymesWith: 'hemenway-observe',
  });
});

test('NEGATIVE PROOF — a create missing a required field is refused before any write', async () => {
  let called = 0;
  const fetchImpl = (async () => { called++; return { ok: true, status: 201, json: async () => ({}) }; }) as unknown as FetchImpl;
  await withClient(fetchImpl, async (c) => {
    let refused: boolean;
    try {
      const r = (await c.callTool({ name: 'chorus_principles_create', arguments: { label: 'Only a label' } })) as { isError?: boolean };
      refused = r.isError === true;
    } catch { refused = true; }
    assert.equal(refused, true);
  });
  assert.equal(called, 0);
});
