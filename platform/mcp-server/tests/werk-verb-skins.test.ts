// #3178 — cut v1, replace with v2, and rename the verb tools to `werk-*`.
// The werk verb MCP tools ARE the verb name (no redundant `chorus_` prefix; a
// verb has no resource, so ADR-031's chorus_<resource>_<verb> never fit them):
//   chorus_pull_card -> werk-pull   (rename; already thin over werk-pull)
//   chorus_commit    -> werk-commit (cut v1 executeCommit/git-queue.sh path)
//   (new)            -> werk-push   (thin over werk-push)
//   (new)            -> werk-accept (thin over werk-accept)
//
// Each test builds the real production server (buildMcpServer) and asserts what
// an MCP client sees / what the commit handler shells. Also verifies hyphens are
// valid tool-name chars (registration/list/call throw otherwise). RED until the
// rename + cut land.

import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';
import { buildMcpServer, type ExecFileAsync } from '../src/server';

const noopExec = (async () => ({ stdout: 'kade/3178', stderr: '' })) as unknown as ExecFileAsync;

test('the four verb tools are named werk-* (no chorus_ prefix on verbs)', async () => {
  const server = buildMcpServer(() => 'kade', { execFileAsync: noopExec, cardsPath: '/fake/cards' });
  const [ct, st] = InMemoryTransport.createLinkedPair();
  const client = new Client({ name: 'werk-skins-test', version: '1.0' });
  await Promise.all([server.connect(st), client.connect(ct)]);
  try {
    const names = (await client.listTools()).tools.map((t) => t.name);
    for (const v of ['werk-pull', 'werk-commit', 'werk-push', 'werk-accept']) {
      assert.ok(names.includes(v), `${v} must be exposed at the MCP surface — #3178`);
    }
  } finally {
    await client.close();
    await server.close();
  }
});

test('the chorus_-prefixed verb tools are gone (v1 names cut, no alias)', async () => {
  const server = buildMcpServer(() => 'kade', { execFileAsync: noopExec, cardsPath: '/fake/cards' });
  const [ct, st] = InMemoryTransport.createLinkedPair();
  const client = new Client({ name: 'werk-skins-test', version: '1.0' });
  await Promise.all([server.connect(st), client.connect(ct)]);
  try {
    const names = (await client.listTools()).tools.map((t) => t.name);
    for (const dead of ['chorus_pull_card', 'chorus_commit']) {
      assert.ok(!names.includes(dead), `${dead} must be removed (renamed to werk-*, no alias) — #3178`);
    }
  } finally {
    await client.close();
    await server.close();
  }
});

test('werk-commit execs the rust werk-commit binary, not the v1 git-queue.sh path', async () => {
  const sink: { file: string }[] = [];
  const capture = (async (file: string) => { sink.push({ file }); return { stdout: 'kade/3178', stderr: '' }; }) as unknown as ExecFileAsync;
  const server = buildMcpServer(() => 'kade', { execFileAsync: capture, cardsPath: '/fake/cards' });
  const [ct, st] = InMemoryTransport.createLinkedPair();
  const client = new Client({ name: 'werk-skins-test', version: '1.0' });
  await Promise.all([server.connect(st), client.connect(ct)]);
  try {
    await client.callTool({ name: 'werk-commit', arguments: { role: 'kade', card_id: 3178, summary: 'test' } });
    const bins = sink.map((c) => c.file);
    assert.ok(bins.some((f) => f.includes('werk-commit')), `werk-commit must exec the werk-commit binary; shelled: ${bins.join(', ')}`);
    assert.ok(!bins.some((f) => f.includes('git-queue')), `werk-commit must NOT touch git-queue.sh (v1 cut); shelled: ${bins.join(', ')}`);
  } finally {
    await client.close();
    await server.close();
  }
});
