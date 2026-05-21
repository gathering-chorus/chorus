// #3025 — collapse the 6 competing cards-cluster MCP tools. The "which tool to
// use" rule must be ENFORCED at the tool boundary, not described in prose.
//
// set-status collapses to ONE enforced path:
//   - chorus_cards_done is the only route to Done (emits card.accepted, DEC-048)
//   - chorus_cards_move refuses status=Done (points at cards_done)
//   - chorus_cards_set refuses any status change (not a structured field here)
// set-metadata (ADR-031): chorus_cards_set is the single writer for descriptive
//   properties INCLUDING the label axes (sequence/domain/chunk). Status is the
//   only thing it refuses — that's a transition, not a field.
// Happy paths still pass straight through to the cards CLI.

import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';
import { buildMcpServer, type ExecFileAsync } from '../src/server';

type Captured = { args: string[] };

function captureExec(sink: Captured[]): ExecFileAsync {
  return (async (_file: string, args: string[]) => {
    sink.push({ args: args ?? [] });
    return { stdout: 'ok', stderr: '' };
  }) as unknown as ExecFileAsync;
}

async function withServer(
  sink: Captured[],
  fn: (client: Client) => Promise<void>,
): Promise<void> {
  const server = buildMcpServer(() => 'wren', {
    execFileAsync: captureExec(sink),
    cardsPath: '/fake/cards',
  });
  const [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();
  const client = new Client({ name: 'cards-collapse-test', version: '1.0' });
  await Promise.all([server.connect(serverTransport), client.connect(clientTransport)]);
  try {
    await fn(client);
  } finally {
    await client.close();
    await server.close();
  }
}

async function callExpectRefusal(client: Client, name: string, args: unknown, match: RegExp): Promise<void> {
  await assert.rejects(
    () => client.callTool({ name, arguments: args as Record<string, unknown> }),
    (err: Error) => { assert.match(err.message, match); return true; },
    `${name} should have refused`,
  );
}

test('cards_move refuses status=Done — must route through cards_done', async () => {
  const sink: Captured[] = [];
  await withServer(sink, async (client) => {
    await callExpectRefusal(client, 'chorus_cards_move', { id: 1, status: 'Done' }, /cards_done/);
  });
  assert.equal(sink.length, 0, 'CLI must not run on a refused move');
});

test('cards_move allows non-Done lanes', async () => {
  const sink: Captured[] = [];
  await withServer(sink, async (client) => {
    await client.callTool({ name: 'chorus_cards_move', arguments: { id: 7, status: 'Next' } });
  });
  assert.deepEqual(sink[0]?.args, ['move', '7', 'Next']);
});

test('cards_set refuses status changes — not a structured field here', async () => {
  const sink: Captured[] = [];
  await withServer(sink, async (client) => {
    await callExpectRefusal(client, 'chorus_cards_set', { id: 1, fields: { status: 'Done' } }, /status/i);
    await callExpectRefusal(client, 'chorus_cards_set', { id: 1, fields: { status: 'WIP' } }, /status/i);
  });
  assert.equal(sink.length, 0, 'CLI must not run on a refused set');
});

test('cards_set accepts label axes — labels are properties (ADR-031)', async () => {
  const sink: Captured[] = [];
  await withServer(sink, async (client) => {
    await client.callTool({ name: 'chorus_cards_set', arguments: { id: 1, fields: { sequence: 'pulse', domain: 'chorus' } } });
  });
  assert.equal(sink[0]?.args[0], 'set');
  assert.ok(sink[0]?.args.includes('sequence=pulse'));
  assert.ok(sink[0]?.args.includes('domain=chorus'));
});

test('cards_set allows structured fields', async () => {
  const sink: Captured[] = [];
  await withServer(sink, async (client) => {
    await client.callTool({ name: 'chorus_cards_set', arguments: { id: 9, fields: { owner: 'wren', priority: 'P1' } } });
  });
  assert.equal(sink[0]?.args[0], 'set');
  assert.ok(sink[0]?.args.includes('owner=wren'));
  assert.ok(sink[0]?.args.includes('priority=P1'));
});

test('cards_done remains the canonical Done path', async () => {
  const sink: Captured[] = [];
  await withServer(sink, async (client) => {
    await client.callTool({ name: 'chorus_cards_done', arguments: { id: 5 } });
  });
  assert.deepEqual(sink[0]?.args, ['done', '5']);
});
