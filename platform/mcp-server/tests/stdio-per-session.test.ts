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

test('profile-bound stdio discovers before enrollment, but authenticates every tool and pins the conversation', async () => {
  const { buildAuthenticatedStdioServer } = await import('../src/main-stdio');
  const { pinProfileBinding } = await import('../src/stdio-session-binding');
  let enrolled = false, expired = false, sessionId = 'session-one', reads = 0, executions = 0;
  const binding = pinProfileBinding('opencode-wren', 'wren', async () => {
    if (!enrolled) throw Error('profile-binding-unavailable');
    return { session_id: sessionId, role: 'wren', principal: 'https://identity.test/wren' };
  });
  const authenticate = async () => {
    reads++;
    const current = await binding();
    if (expired) throw Error('identity-invalid');
    return { mode: 'verified' as const, role: 'wren' as const, principal: current.principal,
      sessionId: current.session_id, token: 'fixture-token', scopes: [] };
  };
  const server = await buildAuthenticatedStdioServer(authenticate, 'wren', true, {
    emitSpineEvent: () => {},
    execFileAsync: async () => { executions++; return { stdout: '{}', stderr: '' }; },
  });
  const [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();
  const client = new Client({ name: 'cold-profile-bridge', version: '1.0' });
  await Promise.all([server.connect(serverTransport), client.connect(clientTransport)]);
  try {
    assert.ok((await client.listTools()).tools.length > 0);
    assert.equal(reads, 0, 'discovery must not look up a nonexistent session');
    const request = { name: 'chorus_cards_view', arguments: { id: 1 } };
    await assert.rejects(client.callTool(request), /profile-binding-unavailable/);
    assert.equal(executions, 0);
    enrolled = true;
    await client.callTool(request);
    assert.equal(executions, 1);
    expired = true;
    await assert.rejects(client.callTool(request), /identity-invalid/);
    assert.equal(executions, 1);
    expired = false; sessionId = 'replacement';
    await assert.rejects(client.callTool(request), /changed-restart-bridge/);
    assert.equal(executions, 1);
    assert.equal(reads, 4);
  } finally { await client.close(); await server.close(); }
});

test('lazy profile enrollment rejects invalid role assertions and never accepts legacy authority', async () => {
  const { buildAuthenticatedStdioServer } = await import('../src/main-stdio');
  let called = 0;
  const legacy = async () => { called++; return { mode: 'legacy-claude' as const, role: 'wren' }; };
  await assert.rejects(buildAuthenticatedStdioServer(legacy, 'jeff', true), /CHORUS_ROLE/);
  assert.equal(called, 0);
  const server = await buildAuthenticatedStdioServer(legacy, 'wren', true);
  const [a,b] = InMemoryTransport.createLinkedPair();
  const client = new Client({ name: 'invalid-profile-authority', version: '1.0' });
  await Promise.all([server.connect(b), client.connect(a)]);
  try {
    await assert.rejects(client.callTool({name:'chorus_cards_view',arguments:{id:1}}),/profile-binding-identity-mismatch/);
  } finally { await client.close(); await server.close(); }
  await assert.rejects(buildAuthenticatedStdioServer(async () => { throw Error('identity-invalid'); }, 'wren', false), /identity-invalid/);
});
