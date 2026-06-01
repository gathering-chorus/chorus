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

test('werk-commit publishes the card-scoped v2 contract — card_id required, no v1 paths', async () => {
  const server = buildMcpServer(() => 'kade', { execFileAsync: noopExec, cardsPath: '/fake/cards' });
  const [ct, st] = InMemoryTransport.createLinkedPair();
  const client = new Client({ name: 'werk-skins-test', version: '1.0' });
  await Promise.all([server.connect(st), client.connect(ct)]);
  try {
    const tool = (await client.listTools()).tools.find((t) => t.name === 'werk-commit');
    const required = (tool?.inputSchema?.required ?? []) as string[];
    const props = Object.keys((tool?.inputSchema?.properties ?? {}) as Record<string, unknown>);
    assert.ok(required.includes('card_id'), 'werk-commit must require card_id — the card-scoped v2 contract, hash 3178');
    assert.ok(!props.includes('paths'), 'werk-commit must NOT accept v1 paths — the git-queue contract is cut, hash 3178');
  } finally {
    await client.close();
    await server.close();
  }
});
