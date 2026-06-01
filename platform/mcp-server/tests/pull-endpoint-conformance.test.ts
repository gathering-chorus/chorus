// #3174 — the pull MCP endpoint conforms to ADR-031 (naming/grain) + ADR-032
// (verb contract). ADR-031's enforcement is a name-test on the tool registry,
// not prose: the bare-verb `chorus_pull` (#2688 read+rebase) is a naming
// violation — a bare verb, not `chorus_<resource>_<verb>`, and NOT one of the
// two allow-listed named transactions ({acp, pull_card}). It must not be exposed
// at the MCP surface.
//
// The single conformant pull surface is `chorus_pull_card` — the allow-listed
// named transaction which since #3135 is a thin skin that execs the rust
// `werk-pull` verb (ADR-032 §1: the endpoint points at the zero-dep verb, it
// does not inline-orchestrate).
//
// These tests build the real production server (buildMcpServer) and assert on
// what an MCP client actually sees via listTools. RED until the bare-verb
// `chorus_pull` tool def is removed from the ListTools registry.

import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';
import { buildMcpServer, type ExecFileAsync } from '../src/server';

const noopExec = (async () => ({ stdout: 'ok', stderr: '' })) as unknown as ExecFileAsync;

test('ADR-031: bare-verb chorus_pull is NOT exposed (naming violation, not an allow-listed transaction)', async () => {
  const server = buildMcpServer(() => 'kade', { execFileAsync: noopExec, cardsPath: '/fake/cards' });
  const [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();
  const client = new Client({ name: 'pull-conformance-test', version: '1.0' });
  await Promise.all([server.connect(serverTransport), client.connect(clientTransport)]);
  try {
    const { tools } = await client.listTools();
    const offending = tools.map((t) => t.name).filter((n) => n === 'chorus_pull');
    assert.equal(
      offending.length,
      0,
      `bare-verb 'chorus_pull' must be retired (#3174) — registry still exposes: ${offending.join(', ')}`,
    );
  } finally {
    await client.close();
    await server.close();
  }
});

test('the conformant pull surface chorus_pull_card remains (allow-listed named transaction → execs werk-pull)', async () => {
  const server = buildMcpServer(() => 'kade', { execFileAsync: noopExec, cardsPath: '/fake/cards' });
  const [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();
  const client = new Client({ name: 'pull-conformance-test', version: '1.0' });
  await Promise.all([server.connect(serverTransport), client.connect(clientTransport)]);
  try {
    const { tools } = await client.listTools();
    const names = tools.map((t) => t.name);
    assert.ok(
      names.includes('chorus_pull_card'),
      'chorus_pull_card (the ADR-031 allow-listed pull transaction, ADR-032 skin over werk-pull) must remain',
    );
  } finally {
    await client.close();
    await server.close();
  }
});
