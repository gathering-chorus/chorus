// #3020 — server uniformity ("no per-type exception"): chorus-mcp must run as
// a stdio-per-session process, not only a shared HTTP daemon. This is the
// "server" leg of the WERK_ROLE_BIN model — a server spawns from PATH like a
// binary/script. Sound because the server is stateless by design (#2949: the
// HTTP transport builds a fresh server per request, no sessions map).
//
// AC covered: the stdio entry builds a working chorus-mcp server whose
//   tools/list returns the chorus_* surface — same buildMcpServer the HTTP
//   transport uses, just handed a different transport. (The end-to-end spawn
//   over a real stdio pipe is the /demo watch-it-work, not this unit test.)
import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';
import { buildStdioServer } from '../src/main-stdio';

test('stdio entry builds a working chorus-mcp server: chorus_ tools enumerate (#3020 server leg)', async () => {
  const server = buildStdioServer('silas');
  const [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();
  const client = new Client({ name: 'stdio-entry-test', version: '1.0' });
  await Promise.all([server.connect(serverTransport), client.connect(clientTransport)]);
  try {
    const { tools } = await client.listTools();
    const names = tools.map((t) => t.name);
    assert.ok(tools.length > 0, 'expected the stdio entry to expose tools');
    assert.ok(
      names.some((n) => n.startsWith('chorus_')),
      `expected chorus_* tools from the stdio entry; got: ${names.slice(0, 5).join(', ')}`,
    );
  } finally {
    await client.close();
    await server.close();
  }
});

test('stdio entry fails loud on missing role — no silent DEPLOY_ROLE default (#3020, Kade gate flag)', () => {
  const saved = process.env.CHORUS_ROLE;
  delete process.env.CHORUS_ROLE;
  try {
    // No explicit role + no CHORUS_ROLE must throw, not silently attribute to a
    // default — the server hosts commit/acp/nudge, which depend on correct role.
    assert.throws(() => buildStdioServer(), /CHORUS_ROLE/);
  } finally {
    if (saved !== undefined) process.env.CHORUS_ROLE = saved;
  }
});
