// #3319 — loom-gemba at the MCP surface. The observation verb follows the
// werk-verb skin pattern (#3110/#3178): the MCP tool IS the verb name, a thin
// skin that execs ~/.chorus/bin/loom-gemba and returns {ok, stdout, ...} with
// the banner+turns text verbatim in stdout. Agent sessions invoke the sense
// through MCP, not raw bash — the skill layer has nothing left to skip.

import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';
import { buildMcpServer, type ExecFileAsync } from '../src/server';

const noopExec = (async () => ({ stdout: '', stderr: '' })) as unknown as ExecFileAsync;

async function withClient(fn: (client: Client) => Promise<void>) {
  const server = buildMcpServer(() => 'wren', { execFileAsync: noopExec, cardsPath: '/fake/cards' });
  const [ct, st] = InMemoryTransport.createLinkedPair();
  const client = new Client({ name: 'loom-gemba-test', version: '1.0' });
  await Promise.all([server.connect(st), client.connect(ct)]);
  try {
    await fn(client);
  } finally {
    await client.close();
    await server.close();
  }
}

test('loom-gemba is exposed at the MCP surface (verb-named, no chorus_ prefix)', async () => {
  await withClient(async (client) => {
    const names = (await client.listTools()).tools.map((t) => t.name);
    assert.ok(names.includes('loom-gemba'), 'loom-gemba must be an MCP tool — #3319 AC');
  });
});

test('loom-gemba contract: role + target (both role-enums), no card_id — observation is not card-scoped', async () => {
  await withClient(async (client) => {
    const tool = (await client.listTools()).tools.find((t) => t.name === 'loom-gemba');
    assert.ok(tool, 'tool def present');
    const schema = tool!.inputSchema as {
      properties?: Record<string, { enum?: string[] }>;
      required?: string[];
    };
    assert.deepEqual(schema.required?.sort(), ['role', 'target'], 'role + target required, nothing else');
    assert.ok(!('card_id' in (schema.properties ?? {})), 'observation has no card_id');
    for (const field of ['role', 'target'] as const) {
      assert.deepEqual(
        schema.properties?.[field]?.enum?.sort(),
        ['kade', 'silas', 'wren'],
        `${field} is the role enum`,
      );
    }
  });
});

test('loom-gemba rejects malformed args through the zod seam (wiring exists, not just a listing)', async () => {
  await withClient(async (client) => {
    // target missing — the dispatch case must parse and refuse, proving the
    // CallTool path is wired, not only the tool list. The zod refusal
    // surfaces as a protocol-level McpError (the throw in the case arm).
    await assert.rejects(
      client.callTool({ name: 'loom-gemba', arguments: { role: 'wren' } }),
      /Invalid arguments/i,
      'missing target must be refused at the zod seam',
    );
  });
});
