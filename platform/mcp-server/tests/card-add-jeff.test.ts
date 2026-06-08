// #2996 — chorus_card_add_jeff must live on the agent-facing MCP server
// (platform/mcp-server, :3341), not chorus-api. The /card skill calls
// mcp__chorus-api__chorus_card_add_jeff; "chorus-api" in .mcp.json points at
// :3341. The tool was originally added to platform/api by mistake, so it was
// never in the live registry. These tests pin it to this server.
//
// AC: tool appears in tools/list; invoking it spawns the cards CLI with
//     DEPLOY_ROLE=jeff (bouncer's isAgent=false → no approval-ask) and --quick
//     (skips the six-section gate). The calling role does NOT leak into
//     attribution — jeff is hardcoded regardless of who invokes.

import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';
import { buildMcpServer, type ExecFileAsync } from '../src/server';

type Captured = { args: string[]; env: NodeJS.ProcessEnv };

function captureExec(sink: Captured[]): ExecFileAsync {
  return (async (_file: string, args: string[], opts?: { env?: NodeJS.ProcessEnv }) => {
    sink.push({ args: args ?? [], env: opts?.env ?? {} });
    return { stdout: 'Created card #9999', stderr: '' };
  }) as unknown as ExecFileAsync;
}

async function withServer(
  role: string,
  sink: Captured[],
  fn: (client: Client) => Promise<void>,
): Promise<void> {
  const server = buildMcpServer(() => role, {
    execFileAsync: captureExec(sink),
    cardsPath: '/fake/cards',
  });
  const [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();
  const client = new Client({ name: 'card-add-jeff-test', version: '1.0' });
  await Promise.all([server.connect(serverTransport), client.connect(clientTransport)]);
  try {
    await fn(client);
  } finally {
    await client.close();
    await server.close();
  }
}

test('chorus_card_add_jeff is in the agent MCP tool registry (#2996)', async () => {
  await withServer('wren', [], async (client) => {
    const { tools } = await client.listTools();
    const names = tools.map((t) => t.name);
    assert.ok(
      names.includes('chorus_card_add_jeff'),
      `chorus_card_add_jeff missing from live tool list. Present: ${names.join(', ')}`,
    );
  });
});

test('chorus_card_add_jeff spawns cards with DEPLOY_ROLE=jeff and NO --quick (#2996, #3293)', async () => {
  const sink: Captured[] = [];
  // DEC-1674: exercise real behavior — invoke the production buildMcpServer directly.
  // Invoke as wren — attribution must still be jeff, not the caller.
  const server = buildMcpServer(() => 'wren', {
    execFileAsync: captureExec(sink),
    cardsPath: '/fake/cards',
  });
  const [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();
  const client = new Client({ name: 'card-add-jeff-test', version: '1.0' });
  await Promise.all([server.connect(serverTransport), client.connect(clientTransport)]);
  try {
    await client.callTool({
      name: 'chorus_card_add_jeff',
      arguments: {
        title: 'test card via jeff path',
        owner: 'silas',
        priority: 'P2',
        domain: 'chorus',
        type: 'fix',
        origin: 'reactive',
      },
    });
  } finally {
    await client.close();
  }

  assert.equal(sink.length, 1, 'expected exactly one cards CLI spawn');
  const call = sink[0];
  assert.equal(
    call.env.DEPLOY_ROLE,
    'jeff',
    `DEPLOY_ROLE must be hardcoded jeff (got '${call.env.DEPLOY_ROLE}') — this is what makes the bouncer skip`,
  );
  // #3293: --quick is gone — every card (incl Jeff-initiated) carries the Experience+AC floor.
  assert.ok(!call.args.includes('--quick'), 'must NOT pass --quick (#3293 removed it; floor is universal)');
  assert.ok(call.args.includes('add'), 'must invoke the cards add verb');
  // The card owner is still honored as a field, distinct from attribution.
  const ownerIdx = call.args.indexOf('--owner');
  assert.ok(ownerIdx >= 0 && call.args[ownerIdx + 1] === 'silas', 'owner field should pass through as silas');
});
