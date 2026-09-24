// @test-type: unit
import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';
import { buildMcpServer, executeNudge, type ExecFileAsync, type FetchImpl } from '../src/server';
import { authenticateAgentRequest, apiIdentityVerifier, authorizeAgentTool, requestEnvironment, withAgentIdentity, type AgentIdentity, type AgentAuthDeps } from '../src/request-identity';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const verified = (role: 'wren' | 'silas' | 'kade' | 'jeff'): AgentIdentity => ({ mode: 'verified', role, principal: `https://identity.test/${role}`, scopes: [], token: `${role}-token`, sessionId: `${role}-session` });
const deps: AgentAuthDeps = { mode: 'legacy-claude', legacyRole: 'silas', verify: async () => ({ principal: 'https://identity.test/wren', role: 'wren', scopes: [] }),
  session: async (id) => ({ session_id: id, principal: 'https://identity.test/wren', role: 'wren', state: 'idle' }) };

test('only the explicit compatibility lane accepts role headers without credentials', async () => {
  assert.deepEqual(await authenticateAgentRequest({ role: 'kade' }, deps), { mode: 'legacy-claude', role: 'kade' });
  for (const request of [{ role: 'kade', sessionId: 'enrolled-1' }, { authorization: 'bad', role: 'kade' }]) {
    await assert.rejects(authenticateAgentRequest(request, deps), /authn-missing/);
  }
  await assert.rejects(authenticateAgentRequest({ role: 'kade' }, { ...deps, mode: 'strict' }), /authn-missing/);
});

test('verified identity replaces role assertions and bad credentials never fall back', async () => {
  const actor = await authenticateAgentRequest({ authorization: 'Bearer wren-token', sessionId: 'one' }, deps);
  assert.equal(actor.role, 'wren');
  await assert.rejects(authenticateAgentRequest({ authorization: 'Bearer wren-token', role: 'jeff' }, deps), /role-mismatch/);
  await assert.rejects(authenticateAgentRequest({ authorization: 'Bearer bad' }, { ...deps, verify: async () => { throw Error('offline'); } }), /identity-unavailable/);
  await assert.rejects(authenticateAgentRequest({ authorization: 'Bearer wren-token', sessionId: '../bad' }, deps), /invalid-session-id/);
});

test('API identity requests are bounded, redirect-safe, and never carry caller role', async () => {
  let seen: RequestInit | undefined;
  const verify = apiIdentityVerifier('http://localhost:3340', (async (_url, init) => {
    seen = init;
    return { ok: true, json: async () => ({ ok: true, principal: 'https://identity.test/wren', role: 'wren', scopes: [] }) } as Response;
  }) as typeof fetch);
  assert.equal((await verify('one-token')).role, 'wren');
  assert.deepEqual(seen?.headers, { Authorization: 'Bearer one-token' });
  assert.equal(seen?.redirect, 'error');
  assert.ok(seen?.signal);
  assert.throws(() => apiIdentityVerifier('http://untrusted.example'), /HTTPS or loopback/);
});

test('enrolled identity binds to the live session principal and role, never to a caller-supplied label', async () => {
  const request = { authorization: 'Bearer wren-token', sessionId: 'one' };
  await assert.rejects(authenticateAgentRequest(request, { ...deps, session: undefined }), /session-verification-unavailable/);
  for (const patch of [{ principal: 'https://identity.test/other' }, { role: 'silas' }, { session_id: 'other' }, { state: 'stopped' }, { state: 'disconnected' }]) {
    await assert.rejects(authenticateAgentRequest(request, { ...deps, session: async () => ({ session_id: 'one', principal: 'https://identity.test/wren', role: 'wren', state: 'idle', ...patch }) }), /session-identity-mismatch/);
  }
  await assert.rejects(authenticateAgentRequest(request, { ...deps, session: async () => { throw Error('missing session'); } }), /session-verification-unavailable/);
});

test('request environments isolate concurrent credentials and strip the daemon session', async () => {
  const ambient = { PATH: '/bin', CHORUS_IDENTITY_TOKEN: 'daemon-secret', CHORUS_SESSION_TOKEN_FILE: '/daemon/token', CHORUS_SESSION_ID: 'daemon', CHORUS_ACTOR_WEBID: 'daemon' };
  const [wren, silas] = await Promise.all(['wren', 'silas'].map((role) => withAgentIdentity(verified(role as 'wren' | 'silas'), async () => {
    await new Promise((resolve) => setTimeout(resolve, role === 'wren' ? 10 : 1));
    return requestEnvironment(ambient);
  })));
  assert.equal(wren.CHORUS_IDENTITY_TOKEN, 'wren-token');
  assert.equal(silas.CHORUS_IDENTITY_TOKEN, 'silas-token');
  assert.equal(wren.CHORUS_SESSION_TOKEN_FILE, undefined);
  assert.equal(wren.CHORUS_SESSION_ID, 'wren-session');
  assert.deepEqual(withAgentIdentity({ mode: 'legacy-claude', role: 'wren' }, () => requestEnvironment(ambient)), { PATH: '/bin', CHORUS_ROLE: 'wren', DEPLOY_ROLE: 'wren' });
  assert.equal(ambient.CHORUS_IDENTITY_TOKEN, 'daemon-secret', 'never mutate process environment');
});

test('mutation actor, human operations, and GO identity cannot be forged', () => {
  assert.throws(() => authorizeAgentTool(verified('wren'), 'werk-pull', { role: 'silas' }), /actor-role-mismatch/);
  assert.doesNotThrow(() => authorizeAgentTool(verified('wren'), 'chorus_wip', { role: 'silas' }));
  assert.throws(() => authorizeAgentTool(verified('wren'), 'chorus_card_add_jeff', {}), /human-authorization-required/);
  assert.doesNotThrow(() => authorizeAgentTool(verified('jeff'), 'chorus_card_add_jeff', { owner: 'silas' }));
  assert.throws(() => authorizeAgentTool(verified('wren'), 'chorus_werk', { role: 'wren', go: true, accepter: 'jeff' }), /accepter-identity-mismatch/);
  assert.doesNotThrow(() => authorizeAgentTool(verified('wren'), 'werk-accept', { role: 'silas' }));
  assert.throws(() => authorizeAgentTool(verified('silas'), 'werk-accept', { role: 'silas' }), /acceptance-authorization-required/);
  assert.throws(() => authorizeAgentTool(verified('kade'), 'chorus_werk', { role: 'kade', go: true, accepter: 'kade' }), /acceptance-authorization-required/);
});

async function clientFor(identity: AgentIdentity, exec: ExecFileAsync): Promise<{ client: Client; close: () => Promise<void> }> {
  const server = buildMcpServer(() => 'jeff', { identity, execFileAsync: exec, cardsPath: '/fake/cards' });
  const [a, b] = InMemoryTransport.createLinkedPair();
  const client = new Client({ name: 'identity-test', version: '1' });
  await Promise.all([server.connect(b), client.connect(a)]);
  return { client, close: async () => { await client.close(); await server.close(); } };
}

test('real MCP dispatch propagates the verified request to child commands concurrently', async () => {
  const captured: NodeJS.ProcessEnv[] = [];
  const exec: ExecFileAsync = async (_file, _args, opts) => {
    await new Promise((resolve) => setTimeout(resolve, opts.env?.DEPLOY_ROLE === 'wren' ? 10 : 1));
    captured.push(opts.env ?? {});
    return { stdout: '{}', stderr: '' };
  };
  const a = await clientFor(verified('wren'), exec);
  const b = await clientFor(verified('silas'), exec);
  try {
    await Promise.all([a.client.callTool({ name: 'chorus_cards_view', arguments: { id: 1 } }), b.client.callTool({ name: 'chorus_cards_view', arguments: { id: 2 } })]);
    assert.equal(captured.length, 2);
    for (const env of captured) {
      assert.equal(env.CHORUS_IDENTITY_TOKEN, `${env.DEPLOY_ROLE}-token`);
      assert.equal(env.CHORUS_SESSION_ID, `${env.DEPLOY_ROLE}-session`);
      assert.equal(env.CHORUS_SESSION_TOKEN_FILE, undefined);
    }
  } finally { await a.close(); await b.close(); }
});

test('direct human-only MCP calls are denied before any child process starts', async () => {
  let called = false;
  const a = await clientFor(verified('wren'), async () => { called = true; return { stdout: '', stderr: '' }; });
  try {
    const args = { title: 'untrusted', owner: 'silas', priority: 'P2', domain: 'chorus', type: 'fix', origin: 'reactive' };
    await assert.rejects(a.client.callTool({ name: 'chorus_card_add_jeff', arguments: args }), /human-authorization-required/);
    assert.equal(called, false);
  } finally { await a.close(); }
});

test('cross-builder acceptance attributes the verified actor even when the daemon resolver claims Jeff', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'mcp-actor-'));
  const previous = process.env.CHORUS_BIN;
  process.env.CHORUS_BIN = dir;
  writeFileSync(join(dir, 'werk-accept'), `#!${process.execPath}\nprocess.stdout.write(JSON.stringify({actor:process.env.DEPLOY_ROLE,principal:process.env.CHORUS_ACTOR_WEBID}));\n`, { mode: 0o700 });
  const a = await clientFor(verified('wren'), async () => ({ stdout: '', stderr: '' }));
  try {
    const result = await a.client.callTool({ name: 'werk-accept', arguments: { role: 'silas', card_id: 1 } });
    const outer = JSON.parse((result.content as Array<{text:string}>)[0].text);
    assert.deepEqual(JSON.parse(outer.stdout), { actor: 'wren', principal: 'https://identity.test/wren' });
  } finally {
    await a.close();
    if (previous === undefined) delete process.env.CHORUS_BIN; else process.env.CHORUS_BIN = previous;
    rmSync(dir, { recursive: true, force: true });
  }
});

test('nudge source session comes only from verified request identity, never model arguments or the legacy role', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'mcp-source-'));
  const previous = process.env.CHORUS_LOG_FILE;
  process.env.CHORUS_LOG_FILE = join(dir, 'spine.log');
  const posted: Record<string, unknown>[] = [];
  const fetcher: FetchImpl = async (url, init) => {
    if (url.includes('properties/resolve')) return { ok: true, json: async () => ({ value: 100 }) };
    posted.push(JSON.parse(init?.body ?? '{}'));
    return { ok: true, json: async () => ({ resolved: 'agent:target', status: 'queued' }) };
  };
  const args = { to: 'kade', message: 'reply', source_session_id: 'forged', target_session_id: 'target' };
  try {
    await withAgentIdentity(verified('wren'), () => executeNudge(args as never, 'wren', fetcher));
    await withAgentIdentity({ mode: 'legacy-claude', role: 'wren' }, () => executeNudge(args as never, 'wren', fetcher));
    assert.equal(posted[0].source_session_id, 'wren-session');
    assert.equal(posted[0].target_session_id, 'target');
    assert.equal(posted[1].source_session_id, undefined);
  } finally {
    if (previous === undefined) delete process.env.CHORUS_LOG_FILE; else process.env.CHORUS_LOG_FILE = previous;
    rmSync(dir, { recursive: true, force: true });
  }
});
