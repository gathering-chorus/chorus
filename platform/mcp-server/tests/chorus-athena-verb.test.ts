// @test-type: unit — signal is fixture-data: in-memory MCP transport + a stub act on CHORUS_ACT_BIN + tmp runsDir (no live act/werk/store)
// #4186 — the MODEL pipeline as one MCP verb (chorus_athena). werk.yml's athena-werk
// and athena-land steps call it, so act never runs inside act, and a role can run
// the model pipeline alone. The stub act records its argv and answers 0 or 1.
import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';
import { buildMcpServer } from '../src/server';

function stubAct(dir: string, exit: number): string {
  const bin = path.join(dir, 'act');
  fs.writeFileSync(bin, `#!/bin/bash\necho "act $*" >> "${dir}/argv.txt"\necho "stub act ran"\n[ "${exit}" = "0" ] || echo "::error::athena: stub says no" >&2\nexit ${exit}\n`);
  fs.chmodSync(bin, 0o755);
  return bin;
}

async function withServer(fn: (client: Client, dir: string) => Promise<void>, exit = 0) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'athena-verb-'));
  const prevAct = process.env.CHORUS_ACT_BIN;
  process.env.CHORUS_ACT_BIN = stubAct(dir, exit);
  const server = buildMcpServer(() => 'wren', { runsDir: dir, cardsPath: '/fake/cards' });
  const [ct, st] = InMemoryTransport.createLinkedPair();
  const client = new Client({ name: 'chorus-athena-test', version: '1.0' });
  await Promise.all([server.connect(st), client.connect(ct)]);
  try { await fn(client, dir); } finally {
    await client.close(); await server.close();
    if (prevAct === undefined) delete process.env.CHORUS_ACT_BIN; else process.env.CHORUS_ACT_BIN = prevAct;
  }
}

test('chorus_athena is an MCP tool and requires role, card_id and target', async () => {
  await withServer(async (client) => {
    const tool = (await client.listTools()).tools.find((t) => t.name === 'chorus_athena');
    assert.ok(tool, 'chorus_athena must be exposed — #4186');
    const required = (tool?.inputSchema?.required ?? []) as string[];
    for (const k of ['role', 'card_id', 'target']) assert.ok(required.includes(k), `${k} required`);
  });
});

test('chorus_athena runs athena.yml with the target and landed sha as inputs, host-native, and reports proven', async () => {
  await withServer(async (client, dir) => {
    const res = await client.callTool({ name: 'chorus_athena', arguments: { role: 'wren', card_id: 4186, target: 'canonical', landed_commit: 'abc123' } });
    const text = (res.content as Array<{ text: string }>)[0].text;
    const body = JSON.parse(text);
    assert.equal(body.ok, true); assert.equal(body.phase, 'proven'); assert.equal(body.target, 'canonical');
    const argv = fs.readFileSync(path.join(dir, 'argv.txt'), 'utf8');
    assert.ok(argv.includes('workflow_dispatch'));
    assert.ok(argv.includes('.github/workflows/athena.yml'), 'runs the canonical athena.yml');
    assert.ok(argv.includes('-P macos-latest=-self-hosted'));
    assert.ok(argv.includes('--input target=canonical'));
    assert.ok(argv.includes('--input landed_commit=abc123'));
    assert.ok(argv.includes('--input card_id=4186') && argv.includes('--input role=wren'));
    assert.ok(fs.existsSync(body.log), 'the run log exists');
  });
});

test('NEGATIVE PROOF — a red athena run is a thrown, named failure, never ok:true', async () => {
  await withServer(async (client) => {
    await assert.rejects(
      client.callTool({ name: 'chorus_athena', arguments: { role: 'wren', card_id: 4186, target: 'werk-rows' } }),
      (e: Error) => /chorus_athena-fail/.test(e.message) && /target=werk-rows/.test(e.message) && /stub says no/.test(e.message),
      'a red run is a thrown, named failure carrying the workflow\'s own error line',
    );
  }, 1);
});

test('NEGATIVE PROOF — an unknown target is refused before act runs', async () => {
  await withServer(async (client, dir) => {
    await assert.rejects(
      client.callTool({ name: 'chorus_athena', arguments: { role: 'wren', card_id: 4186, target: 'prod' } }),
      /Invalid arguments/,
    );
    assert.ok(!fs.existsSync(path.join(dir, 'argv.txt')), 'act never ran');
  });
});
