// @test-type: unit — in-memory MCP transport + a stub chorus-crawl on CHORUS_BIN
//
// #4405 reopen 4 — one runner for the sensors (Jeff, 2026-10-12: "make sure
// our runner is properly aligned integrated and visible as a sensor"; 19:39:
// "mcp is our runner for services and apis"). launchd and a person or agent
// call the same tool, herald_run, which runs the one chorus-crawl binary with
// the sensor named. Before this there was no way to run a sensor except a
// shell or a plist.
import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';
import { buildMcpServer } from '../src/server';

async function withServer(stub: string, fn: (client: Client) => Promise<void>) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'herald-run-'));
  const bin = path.join(dir, 'chorus-crawl');
  fs.writeFileSync(bin, stub);
  fs.chmodSync(bin, 0o755);
  const prev = process.env.CHORUS_BIN; process.env.CHORUS_BIN = dir;
  const server = buildMcpServer(() => 'silas', { runsDir: dir, cardsPath: '/fake/cards' });
  const [ct, st] = InMemoryTransport.createLinkedPair();
  const client = new Client({ name: 'herald-run-test', version: '1.0' });
  await Promise.all([server.connect(st), client.connect(ct)]);
  try { await fn(client); } finally {
    await client.close(); await server.close();
    if (prev === undefined) delete process.env.CHORUS_BIN; else process.env.CHORUS_BIN = prev;
  }
}

const ECHO = '#!/bin/bash\necho "ran=chorus-crawl args=$*"\n';

async function run(client: Client, args: Record<string, unknown>) {
  const res = await client.callTool({ name: 'herald_run', arguments: { role: 'silas', ...args } });
  return JSON.parse((res.content as Array<{ text: string }>)[0].text);
}

test('herald_run is listed, so a session sees the sensors it can run', async () => {
  await withServer(ECHO, async (client) => {
    const { tools } = await client.listTools();
    const t = tools.find((x) => x.name === 'herald_run');
    assert.ok(t, 'no herald_run tool');
    const sensors = ((t!.inputSchema.properties as Record<string, { enum?: string[] }>).sensor.enum) || [];
    assert.deepEqual([...sensors].sort(), ['code', 'files', 'infra', 'services', 'stores']);
  });
});

test('each sensor runs chorus-crawl with its own subcommand', async () => {
  await withServer(ECHO, async (client) => {
    for (const sensor of ['code', 'services', 'stores', 'infra']) {
      const r = await run(client, { sensor });
      assert.equal(r.ok, true, JSON.stringify(r));
      assert.equal(r.sensor, sensor);
      assert.ok(r.tail.includes(`args=${sensor}`), r.tail);
    }
    // the files sensor is the crawler's default pass: no subcommand
    const r = await run(client, { sensor: 'files' });
    assert.ok(/args=\s*$/m.test(r.tail), r.tail);
  });
});

test('the code sensor takes a target and a dry run', async () => {
  await withServer(ECHO, async (client) => {
    const r = await run(client, { sensor: 'code', target: '/repos/wine-diary', dry_run: true });
    assert.ok(r.tail.includes('args=code --target /repos/wine-diary --dry-run'), r.tail);
  });
});

test('NEGATIVE PROOF — a red pass is reported red with its exit and output, never ok', async () => {
  const RED = '#!/bin/bash\necho "code: create 0 · update 0 · delete 0 · unchanged 50"\necho "refused: mass delete" >&2\nexit 3\n';
  await withServer(RED, async (client) => {
    const r = await run(client, { sensor: 'code' });
    assert.equal(r.ok, false, JSON.stringify(r));
    assert.equal(r.exit, 3);
    assert.ok(r.tail.includes('refused: mass delete'), r.tail);
  });
});

test('NEGATIVE PROOF — a sensor the crawler does not have, or a target on one that takes none, is refused', async () => {
  await withServer(ECHO, async (client) => {
    const bad = await client.callTool({ name: 'herald_run', arguments: { role: 'silas', sensor: 'weather' } });
    assert.ok(/invalid arguments/.test((bad.content as Array<{ text: string }>)[0].text));
    const t = await client.callTool({ name: 'herald_run', arguments: { role: 'silas', sensor: 'infra', target: '/x' } });
    const text = (t.content as Array<{ text: string }>)[0].text;
    assert.ok(/only the code sensor takes a target/.test(text), text);
    assert.ok(!text.includes('ran=chorus-crawl'), 'the binary ran anyway');
  });
});
