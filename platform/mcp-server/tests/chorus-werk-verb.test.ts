// #3241 — the werk pipeline as ONE MCP verb (chorus_werk). It wraps the act run of
// werk.yml so a role triggers the whole pipeline via MCP like every other verb — no
// raw `act` CLI surface, no PATH/-P/-W wrangling leaked to the caller. The verb
// encapsulates: canonical werk.yml (-W), host-native runner (-P …=-self-hosted),
// card inputs, and the PATH that makes chorus-mcp-call.sh resolvable.
//
// These tests build the real production server with an injected execFileAsync that
// captures the act invocation, and assert the surface + the encapsulated argv. RED
// until chorus_werk is registered + wired.

import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';
import { buildMcpServer, type ExecFileAsync } from '../src/server';

type Call = { file: string; args: string[]; opts: { env?: Record<string, string> } };

function captureExec(sink: Call[]): ExecFileAsync {
  return (async (file: string, args: string[], opts: { env?: Record<string, string> } = {}) => {
    sink.push({ file, args, opts });
    // act prints the pipeline log to stdout; the landed line carries the accept cmd.
    return { stdout: '[landed] #3241 deployed + LIVE. NOT yet accepted.\n  DEPLOY_ROLE=jeff werk-accept 3241 kade', stderr: '' };
  }) as unknown as ExecFileAsync;
}

async function withServer(fn: (client: Client) => Promise<void>, sink: Call[]) {
  const server = buildMcpServer(() => 'kade', { execFileAsync: captureExec(sink), cardsPath: '/fake/cards' });
  const [ct, st] = InMemoryTransport.createLinkedPair();
  const client = new Client({ name: 'chorus-werk-test', version: '1.0' });
  await Promise.all([server.connect(st), client.connect(ct)]);
  try { await fn(client); } finally { await client.close(); await server.close(); }
}

test('chorus_werk is exposed at the MCP surface (pipeline trigger is a verb, not raw act)', async () => {
  await withServer(async (client) => {
    const names = (await client.listTools()).tools.map((t) => t.name);
    assert.ok(names.includes('chorus_werk'), 'chorus_werk must be an MCP tool — #3241');
  }, []);
});

test('chorus_werk requires role + card_id', async () => {
  await withServer(async (client) => {
    const tool = (await client.listTools()).tools.find((t) => t.name === 'chorus_werk');
    const required = (tool?.inputSchema?.required ?? []) as string[];
    assert.ok(required.includes('role'), 'role required');
    assert.ok(required.includes('card_id'), 'card_id required');
  }, []);
});

test('chorus_werk runs act on CANONICAL werk.yml, host-native, with card inputs (encapsulated)', async () => {
  const sink: Call[] = [];
  await withServer(async (client) => {
    await client.callTool({ name: 'chorus_werk', arguments: { role: 'kade', card_id: 3241, accepter: 'jeff' } });
  }, sink);

  assert.equal(sink.length, 1, 'exactly one subprocess — the act run');
  const call = sink[0];
  assert.match(call.file, /(^|\/)act$/, 'invokes the act binary');
  const a = call.args.join(' ');
  assert.match(a, /workflow_dispatch/, 'workflow_dispatch event');
  assert.match(a, /-W .*\.github\/workflows\/werk\.yml/, 'targets werk.yml');
  assert.match(a, /-P macos-latest=-self-hosted/, 'host-native runner (no docker)');
  assert.match(a, /--input card_id=3241/, 'forwards card_id');
  assert.match(a, /--input role=kade/, 'forwards role');
  assert.match(a, /--input accepter=jeff/, 'forwards accepter');
  // canonical werk.yml, not a per-werk copy (the pipeline config is infra)
  assert.ok(/-W (?!.*chorus-werk).*werk\.yml/.test(a), 'uses canonical werk.yml, not a werk copy');
  // PATH encapsulates chorus-mcp-call.sh (canonical platform/scripts) so the caller needs no symlink
  assert.match(call.opts.env?.PATH ?? '', /platform\/scripts/, 'PATH includes canonical platform/scripts (chorus-mcp-call.sh resolvable)');
});

test('chorus_werk returns the stop-before-accept command, never auto-accepts (DEC-048)', async () => {
  const sink: Call[] = [];
  await withServer(async (client) => {
    const res = await client.callTool({ name: 'chorus_werk', arguments: { role: 'kade', card_id: 3241, accepter: 'jeff' } });
    const text = (res.content as Array<{ type: string; text: string }>).map((c) => c.text).join('\n');
    assert.match(text, /werk-accept 3241 kade/, 'surfaces the accept command for the human');
  }, sink);
  // the verb itself must never invoke werk-accept
  const a = sink.map((c) => c.args.join(' ')).join(' | ');
  assert.ok(!/werk-accept/.test(sink.map((c) => c.file).join(' ')), 'chorus_werk must not exec werk-accept');
});
