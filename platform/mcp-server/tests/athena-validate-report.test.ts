// @test-type: unit — signal is fixture-data: in-memory MCP transport + a stub athena-validate on CHORUS_BIN
// #4187 — a red sweep is an answer with a report, not a thrown tool failure. Jeff,
// 2026-09-16: "use athena-validate to generate and then resolve gaps". Before this the
// MCP tool threw "work-fail exit=1" on every dirty sweep and the gap list was lost.
import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';
import { buildMcpServer, parseValidateSummary } from '../src/server';

function stub(dir: string, body: string) {
  const bin = path.join(dir, 'athena-validate');
  fs.writeFileSync(bin, `#!/bin/bash\n${body}\n`); fs.chmodSync(bin, 0o755);
}

async function withServer(body: string, fn: (client: Client) => Promise<void>) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'athena-validate-'));
  stub(dir, body);
  const prev = process.env.CHORUS_BIN; process.env.CHORUS_BIN = dir;
  const server = buildMcpServer(() => 'wren', { runsDir: dir, cardsPath: '/fake/cards' });
  const [ct, st] = InMemoryTransport.createLinkedPair();
  const client = new Client({ name: 'athena-validate-test', version: '1.0' });
  await Promise.all([server.connect(st), client.connect(ct)]);
  try { await fn(client); } finally {
    await client.close(); await server.close();
    if (prev === undefined) delete process.env.CHORUS_BIN; else process.env.CHORUS_BIN = prev;
  }
}

async function call(client: Client) {
  const res = await client.callTool({ name: 'athena-validate', arguments: { role: 'wren', args: [] } });
  return JSON.parse((res.content as Array<{ text: string }>)[0].text);
}

test('a dirty sweep (exit 1) comes back as verdict=dirty WITH its gap list, never thrown', async () => {
  await withServer('echo "1) retired predicates in use:"; echo "graph-issue|v1-row|urn:chorus:instances|File|14425"; echo "graph-issue|dangling-edge|3"; echo "graph-summary|14428|dirty"; exit 1', async (client) => {
    const body = await call(client);
    assert.equal(body.ok, false); assert.equal(body.verdict, 'dirty'); assert.equal(body.issues, 14428); assert.equal(body.exit, 1);
    assert.deepEqual(body.report, ['graph-issue|v1-row|urn:chorus:instances|File|14425', 'graph-issue|dangling-edge|3', 'graph-summary|14428|dirty']);
  });
});

test('a clean sweep is ok:true with issues 0', async () => {
  await withServer('echo "PROVEN CLEAN"; echo "graph-summary|0|clean"; exit 0', async (client) => {
    const body = await call(client);
    assert.equal(body.ok, true); assert.equal(body.verdict, 'clean'); assert.equal(body.issues, 0);
  });
});

test('NEGATIVE PROOF — an unreachable store is unmeasured, never clean and never a count', async () => {
  await withServer('echo "graph-summary|UNMEASURED|unreachable"; exit 2', async (client) => {
    const body = await call(client);
    assert.equal(body.ok, false); assert.equal(body.verdict, 'unmeasured'); assert.equal(body.issues, null);
  });
});

test('parseValidateSummary reads the script line, not the exit code', () => {
  assert.deepEqual(parseValidateSummary('x\ngraph-summary|7|dirty\n'), { issues: 7, state: 'dirty' });
  assert.deepEqual(parseValidateSummary('graph-summary|UNMEASURED|unreachable'), { issues: null, state: 'unreachable' });
  assert.deepEqual(parseValidateSummary('no summary here'), { issues: null, state: 'unparsed' });
});
