// @test-type: integration
import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import express from 'express';
import { mountMcpEndpoint } from '../src/transport';

test('HTTP transport authenticates before MCP dispatch on all methods', async () => {
  const app = express(); app.use(express.json());
  mountMcpEndpoint(app, { mode: 'strict', verify: async (token) => {
    if (token !== 'wren-token') throw Error('invalid');
    return { principal: 'https://identity.test/wren', role: 'wren', scopes: [] };
  }, session: async (id) => ({ session_id: id, principal: id === 'forged' ? 'https://identity.test/silas' : 'https://identity.test/wren', role: 'wren', state: 'idle' }) });
  const server = app.listen(0, '127.0.0.1');
  await new Promise<void>((resolve) => server.once('listening', resolve));
  const addr = server.address(); assert.ok(addr && typeof addr !== 'string');
  const url = `http://127.0.0.1:${addr.port}/mcp`;
  try {
    for (const method of ['POST', 'GET', 'DELETE']) {
      const response = await fetch(url, { method, headers: { 'X-Chorus-Role': 'jeff' } });
      assert.equal(response.status, 401); await response.text();
    }
    const mismatch = await fetch(url, { method: 'DELETE', headers: { Authorization: 'Bearer wren-token', 'X-Chorus-Role': 'jeff' } });
    assert.equal(mismatch.status, 403); await mismatch.text();
    const forged = await fetch(url, { method: 'DELETE', headers: { Authorization: 'Bearer wren-token', 'X-Chorus-Session-Id': 'forged' } });
    assert.equal(forged.status, 403); await forged.text();
    const valid = await fetch(url, { method: 'DELETE', headers: { Authorization: 'Bearer wren-token', 'X-Chorus-Session-Id': 'session-1' } });
    assert.equal(valid.status, 204);
  } finally { server.closeAllConnections(); await new Promise<void>((resolve) => server.close(() => resolve())); }
});
