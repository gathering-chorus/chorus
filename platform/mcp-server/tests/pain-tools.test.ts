// #3029 — the pain board exists as an HTTP page (/borg/pain.html) + chorus-api
// endpoints, but the TEAM consumes the spine through MCP tools, not browser tabs.
// Without chorus_pain_rollup / chorus_pain_card registered, the three roles —
// the ones actually in the death spiral — cannot see their pain in aggregate
// from inside a session. These tools are the missing surface.
//
// Boundary: MCP is the API gateway (Integrations domain). The rollup logic lives
// once, in chorus-api (handlers/logs-query.queryPainRollup). These tools PROXY
// the existing /api/chorus/pain/* endpoints via injectable fetchImpl — no second
// copy of the rollup logic, and provably the same numbers the page shows.

import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import type { Server } from '@modelcontextprotocol/sdk/server/index.js';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';
import { buildMcpServer, type FetchImpl, type McpServerDeps } from '../src/server';

type Hit = { url: string; init?: unknown };

function captureFetch(sink: Hit[], body: unknown): FetchImpl {
  return (async (url: string, init?: unknown) => {
    sink.push({ url, init });
    return { ok: true, status: 200, json: async () => body, text: async () => JSON.stringify(body) };
  }) as unknown as FetchImpl;
}

// Inert non-fetch deps so buildMcpServer stands up without touching cards/git.
const inertDeps: Partial<McpServerDeps> = {
  execFileAsync: (async () => ({ stdout: '', stderr: '' })) as never,
  cardsPath: '/fake/cards',
  emitSpineEvent: () => {},
};

async function connect(server: Server): Promise<Client> {
  const [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();
  const client = new Client({ name: 'pain-tools-test', version: '1.0' });
  await Promise.all([server.connect(serverTransport), client.connect(clientTransport)]);
  return client;
}

const textOf = (res: unknown) =>
  ((res as { content: Array<{ type: string; text: string }> }).content[0].text);

test('chorus_pain_rollup is registered and proxies /api/chorus/pain/rollup with the window param', async () => {
  const hits: Hit[] = [];
  const rollup = { ok: true, window: '12h', total: 106, byProduct: { Chorus: 83, Gathering: 23 }, classes: [] };
  const server = buildMcpServer(() => 'silas', { ...inertDeps, fetchImpl: captureFetch(hits, rollup), apiBase: 'http://api.test' });
  const client = await connect(server);
  try {
    const names = (await client.listTools()).tools.map((t) => t.name);
    assert.ok(names.includes('chorus_pain_rollup'), 'chorus_pain_rollup must be registered');
    assert.ok(names.includes('chorus_pain_card'), 'chorus_pain_card must be registered');
    const res = await client.callTool({ name: 'chorus_pain_rollup', arguments: { window: '12h' } });
    assert.equal(hits.length, 1, 'tool must hit the api exactly once');
    assert.equal(hits[0].url, 'http://api.test/api/chorus/pain/rollup?window=12h');
    const parsed = JSON.parse(textOf(res));
    assert.equal(parsed.total, 106);
    assert.deepEqual(parsed.byProduct, { Chorus: 83, Gathering: 23 });
  } finally {
    await client.close();
    await server.close();
  }
});

test('chorus_pain_rollup defaults the window when omitted', async () => {
  const hits: Hit[] = [];
  const server = buildMcpServer(() => 'silas', { ...inertDeps, fetchImpl: captureFetch(hits, { ok: true, window: '7d', total: 0, classes: [] }), apiBase: 'http://api.test' });
  const client = await connect(server);
  try {
    await client.callTool({ name: 'chorus_pain_rollup', arguments: {} });
    assert.equal(hits[0].url, 'http://api.test/api/chorus/pain/rollup?window=7d');
  } finally {
    await client.close();
    await server.close();
  }
});

test('chorus_pain_card proxies /api/chorus/pain/card/:id', async () => {
  const hits: Hit[] = [];
  const cardTrace = { ok: true, events: [{ event: 'card.pulled', trace_id: 'abc' }], count: 1 };
  const server = buildMcpServer(() => 'silas', { ...inertDeps, fetchImpl: captureFetch(hits, cardTrace), apiBase: 'http://api.test' });
  const client = await connect(server);
  try {
    const res = await client.callTool({ name: 'chorus_pain_card', arguments: { card_id: 3029 } });
    assert.equal(hits[0].url, 'http://api.test/api/chorus/pain/card/3029');
    const parsed = JSON.parse(textOf(res));
    assert.equal(parsed.count, 1);
    assert.equal(parsed.events[0].event, 'card.pulled');
  } finally {
    await client.close();
    await server.close();
  }
});

test('chorus_pain_card refuses a non-numeric card_id at the boundary', async () => {
  const server = buildMcpServer(() => 'silas', { ...inertDeps, fetchImpl: captureFetch([], {}), apiBase: 'http://api.test' });
  const client = await connect(server);
  try {
    await assert.rejects(
      () => client.callTool({ name: 'chorus_pain_card', arguments: { card_id: 'nope' as unknown as number } }),
      /Invalid arguments|card_id/,
    );
  } finally {
    await client.close();
    await server.close();
  }
});
