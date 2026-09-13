// @test-type: contract:api
// #4149 — chorus_logs_query is retired. Two raw-LogQL doors to the same Loki is
// a competing implementation, and ours was the weaker of the two: it takes the
// same LogQL string as Grafana's query_loki_logs but ships no way to discover
// the labels, so the caller has to guess the schema. mcp-grafana answers
// list_loki_label_names / list_loki_label_values, so it wins the raw door.
//
// What stays is the half that is NOT a duplicate: the noun tools. They take a
// card number, a trace id, a branch — the shape we actually ask questions in —
// and need no label schema at all. Those are single-sourced here and nowhere
// else.
//
// The retired name fails LOUD rather than vanishing: a caller who reaches for
// it is told where the raw door moved, instead of getting "unknown tool".

import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';
import { buildMcpServer, type ExecFileAsync } from '../src/server';

const noopExec = (async () => ({ stdout: 'ok', stderr: '' })) as unknown as ExecFileAsync;

async function withServer(fn: (client: Client) => Promise<void>): Promise<void> {
  const server = buildMcpServer(() => 'silas', { execFileAsync: noopExec, cardsPath: '/fake/cards' });
  const [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();
  const client = new Client({ name: 'logs-query-retired-test', version: '1.0' });
  await Promise.all([server.connect(serverTransport), client.connect(clientTransport)]);
  try {
    await fn(client);
  } finally {
    await client.close();
    await server.close();
  }
}

async function toolNames(client: Client): Promise<string[]> {
  const { tools } = await client.listTools();
  return tools.map((t) => t.name);
}

test('chorus_logs_query is not offered — the raw LogQL door is mcp-grafana query_loki_logs', async () => {
  await withServer(async (client) => {
    assert.ok(!(await toolNames(client)).includes('chorus_logs_query'));
  });
});

test('NEGATIVE PROOF — the same check sees a tool that IS present', async () => {
  // Without this, "chorus_logs_query is absent" would also pass against an
  // empty or broken tool list, which is the failure mode that lets a retirement
  // test go green for the wrong reason.
  await withServer(async (client) => {
    const names = await toolNames(client);
    assert.ok(names.length > 20, `tool list looks empty: ${names.length}`);
    assert.ok(names.includes('chorus_logs_for_card'));
  });
});

test('the noun tools survive — card, trace, branch, recent errors', async () => {
  await withServer(async (client) => {
    const names = await toolNames(client);
    for (const t of [
      'chorus_logs_for_card',
      'chorus_logs_for_trace',
      'chorus_logs_for_branch',
      'chorus_logs_recent_errors',
    ]) {
      assert.ok(names.includes(t), `${t} must survive the retirement`);
    }
  });
});

test('calling the retired name fails loud and names its replacement', async () => {
  await withServer(async (client) => {
    await assert.rejects(
      () => client.callTool({ name: 'chorus_logs_query', arguments: { query: '{job="chorus-api"}' } }),
      (err: Error) => {
        assert.match(err.message, /query_loki_logs/);
        return true;
      },
      'the retired tool should refuse with a pointer, not a bare unknown-tool error',
    );
  });
});

test('no surviving tool description tells a caller to use chorus_logs_query', async () => {
  await withServer(async (client) => {
    const { tools } = await client.listTools();
    const stale = tools.filter((t) => (t.description ?? '').includes('chorus_logs_query'));
    assert.equal(stale.length, 0, `stale pointers in: ${stale.map((t) => t.name).join(', ')}`);
  });
});
