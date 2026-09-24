// @test-type: integration — local HTTP fixture, injected verifier and UDS transport
import express from 'express';
import type { Server } from 'node:http';
import { canControlAgent, mountAgentSessions, AgentTransport } from '../src/handlers/agent-sessions';
import type { AgentIdentityDeps } from '../src/handlers/agent-identity';

const sessions = [
  { session_id: 'wren-1', role: 'wren', principal: 'https://id.example/wren-opaque', mode: 'managed' },
  { session_id: 'silas-1', role: 'silas', principal: 'https://id.example/silas-opaque', mode: 'native' },
];
let server: Server;
let base: string;
let transport: jest.MockedFunction<AgentTransport>;
beforeEach(async () => {
  const deps: AgentIdentityDeps = {
    verify: async token => ['wren', 'silas', 'jeff'].includes(token)
      ? { ok: true, webId: `https://id.example/${token}-opaque`, scope: [] } : { ok: false, reason: 'invalid' },
    roleForWebId: async webId => ['wren', 'silas', 'jeff'].find(r => webId === `https://id.example/${r}-opaque`) ?? null,
  };
  transport = jest.fn<ReturnType<AgentTransport>, Parameters<AgentTransport>>(async (method, route) => {
    if (route === '/v1/sessions') return { version: 1, sessions };
    const session = sessions.find(s => route === `/v1/sessions/${s.session_id}`);
    if (session) return session;
    return method === 'POST' ? { status: 'transport_accepted' } : { events: [] };
  });
  const app = express(); app.use(express.json()); mountAgentSessions(app, deps, transport);
  await new Promise<void>(resolve => { server = app.listen(0, '127.0.0.1', resolve); });
  const address = server.address();
  if (!address || typeof address === 'string') throw new Error('missing test address');
  base = `http://127.0.0.1:${address.port}/api/chorus/agent-sessions`;
});
afterEach(async () => { await new Promise<void>((resolve, reject) => server.close(err => err ? reject(err) : resolve())); });
async function call(token: string, path = '', body?: Record<string, unknown>) {
  const response = await fetch(base + path, { method: body ? 'POST' : 'GET', headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' }, body: body ? JSON.stringify(body) : undefined });
  return { status: response.status, cache: response.headers.get('cache-control'), body: await response.json() as Record<string, unknown> };
}
const input = { version: 1, message_id: 'test:1', input: 'hello', kind: 'peer_message' };

test('listing authenticates each concurrent request and filters by both principal and role', async () => {
  const [wren, silas, jeff, invalid] = await Promise.all([call('wren'), call('silas'), call('jeff'), call('invalid')]);
  expect(wren.body.sessions).toEqual([sessions[0]]);
  expect(silas.body.sessions).toEqual([sessions[1]]);
  expect(jeff.body.sessions).toEqual(sessions);
  expect(wren.cache).toBe('no-store');
  expect(invalid.status).toBe(401);
  expect(canControlAgent({ principal: sessions[0].principal, role: 'silas' }, sessions[0])).toBe(false);
  expect(canControlAgent({ principal: 'https://id.example/imposter', role: 'wren' }, sessions[0])).toBe(false);
});

test('cross-role controls and human-input impersonation refuse before supervisor mutation', async () => {
  expect((await call('silas', '/wren-1/stop', {})).status).toBe(403);
  expect((await call('wren', '/wren-1/send', { ...input, kind: 'human_input' })).status).toBe(403);
  expect(transport.mock.calls.some(([method]) => method === 'POST')).toBe(false);
  expect((await call('jeff', '/wren-1/send', { ...input, kind: 'human_input' })).status).toBe(200);
  expect(transport).toHaveBeenLastCalledWith('POST', '/v1/sessions/wren-1/send', { ...input, kind: 'human_input' });
});

test('native send explicitly refuses the non-durable route, and malformed envelopes never reach send', async () => {
  const native = await call('silas', '/silas-1/send', input);
  expect(native).toMatchObject({ status: 409, body: { error: 'native_requires_pulse_inbox', persisted: false } });
  expect((await call('wren', '/wren-1/send', { ...input, kind: 'context' })).status).toBe(400);
  expect((await call('wren', '/wren-1/send', { ...input, message_id: '../bad' })).status).toBe(400);
  expect(transport.mock.calls.some(([method]) => method === 'POST')).toBe(false);
});

test('managed admission remains transport_accepted; caller credential injection is stripped', async () => {
  const result = await call('wren', '/wren-1/send', { ...input, credential_file: '/evil', principal: 'jeff', role: 'jeff' });
  expect(result.body).toEqual({ status: 'transport_accepted' });
  expect(transport).toHaveBeenLastCalledWith('POST', '/v1/sessions/wren-1/send', input);
  transport.mockRejectedValue(new Error('socket offline'));
  expect((await call('wren')).status).toBe(503);
});
