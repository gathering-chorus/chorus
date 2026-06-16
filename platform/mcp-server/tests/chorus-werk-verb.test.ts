// #3241 — the werk pipeline as ONE MCP verb (chorus_werk). It wraps the act run of
// werk.yml so a role triggers the whole pipeline via MCP like every other verb — no
// raw `act` CLI surface, no PATH/-P/-W wrangling leaked to the caller. The verb
// encapsulates: canonical werk.yml (-W), host-native runner (-P …=-self-hosted),
// card inputs, and the PATH that makes chorus-mcp-call.sh resolvable.
//
// #3458 — the act run is now a DETACHED spawn (async-launch, return-immediately) so the
// MCP call never holds open across the multi-minute run → the transport cannot drop.
// The test seam moved from the awaited execFileAsync to the injected spawnFn; these
// tests capture the spawned (command, args, opts) and assert the SAME encapsulated argv
// + env, plus the new async contract: first call returns phase:"running" and surfaces
// the go-resume command, never auto-accepting (DEC-048). A fresh temp runsDir per server
// keeps run-state off the live ~/.chorus/werk-runs.

import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';
import { buildMcpServer, type SpawnFn } from '../src/server';

type SpawnCall = { command: string; args: string[]; opts: { env?: Record<string, string>; detached?: boolean; stdio?: string } };

function captureSpawn(sink: SpawnCall[]): SpawnFn {
  return ((command: string, args: string[], opts: SpawnCall['opts'] = {}) => {
    sink.push({ command, args, opts });
    // a detached child: the verb only reads .pid (for run-state) and calls .unref().
    return { pid: 4242, unref() {} };
  }) as unknown as SpawnFn;
}

async function withServer(fn: (client: Client) => Promise<void>, sink: SpawnCall[]) {
  const runsDir = fs.mkdtempSync(path.join(os.tmpdir(), 'werk-runs-test-'));
  const server = buildMcpServer(() => 'kade', { spawnFn: captureSpawn(sink), runsDir, cardsPath: '/fake/cards' });
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

test('chorus_werk launches act on CANONICAL werk.yml, host-native, with card inputs — detached (#3458)', async () => {
  const sink: SpawnCall[] = [];
  await withServer(async (client) => {
    await client.callTool({ name: 'chorus_werk', arguments: { role: 'kade', card_id: 3241, accepter: 'jeff' } });
  }, sink);

  assert.equal(sink.length, 1, 'exactly one detached spawn — the act run');
  const call = sink[0];
  // #3458 — act runs inside a `bash -c` wrapper that streams to the per-card log and
  // appends the durable WERK_EXIT sentinel; the act argv lives in the wrapped string.
  assert.equal(call.command, 'bash', 'spawns the act run via a bash -c wrapper');
  assert.equal(call.args[0], '-c', 'bash -c <wrapped act command>');
  const wrapped = call.args[1];
  assert.match(wrapped, /(^|["/])act"? workflow_dispatch/, 'invokes the act binary');
  assert.match(wrapped, /workflow_dispatch/, 'workflow_dispatch event');
  assert.match(wrapped, /-W .*\.github\/workflows\/werk\.yml/, 'targets werk.yml');
  assert.match(wrapped, /-P macos-latest=-self-hosted/, 'host-native runner (no docker)');
  assert.match(wrapped, /--input card_id=3241/, 'forwards card_id');
  assert.match(wrapped, /--input role=kade/, 'forwards role');
  assert.match(wrapped, /--input accepter=jeff/, 'forwards accepter');
  // canonical werk.yml, not a per-werk copy (the pipeline config is infra)
  assert.ok(/-W (?!.*chorus-werk).*werk\.yml/.test(wrapped), 'uses canonical werk.yml, not a werk copy');
  // #3458 — the durable terminal marker that survives an mcp-server restart
  assert.match(wrapped, /WERK_EXIT=\$\?/, 'appends the durable WERK_EXIT sentinel to the log');
  // #3458 — detached + unref: the call returns immediately, nothing held → no transport drop
  assert.equal(call.opts.detached, true, 'detached (async-launch — the MCP call never holds open across the run)');
  // PATH encapsulates chorus-mcp-call.sh (canonical platform/scripts) so the caller needs no symlink
  assert.match(call.opts.env?.PATH ?? '', /platform\/scripts/, 'PATH includes canonical platform/scripts (chorus-mcp-call.sh resolvable)');
});

test('chorus_werk first call returns running + surfaces the go-resume command, never auto-accepts (DEC-048, #3458)', async () => {
  const sink: SpawnCall[] = [];
  let res: { content: Array<{ type: string; text: string }> } | undefined;
  await withServer(async (client) => {
    res = await client.callTool({ name: 'chorus_werk', arguments: { role: 'kade', card_id: 3241, accepter: 'jeff' } }) as typeof res;
  }, sink);
  const text = (res!.content).map((c) => c.text).join('\n');
  // #3458 — async-launch contract: the first call launches detached and returns 'running',
  // it does NOT run synchronously to a presented/landed state in one call.
  assert.match(text, /"phase":"running"/, 'first call launches detached and returns running');
  // the human's GO is a SEPARATE resume invocation (go:true) — surfaced, never auto-run (DEC-048).
  assert.match(text, /go_command/, 'surfaces the go-resume command for the human');
  assert.match(text, /go:true/, 'the resume command carries go:true (Jeff resumes the same pipeline)');
  // the verb itself must never invoke werk-accept — not in the wrapped act command, not as a spawn.
  const spawned = sink.map((c) => `${c.command} ${c.args.join(' ')}`).join(' | ');
  assert.ok(!/werk-accept/.test(spawned), 'chorus_werk must not exec werk-accept (no self-accept)');
});
