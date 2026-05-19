// #3008 — Integration test for Mcp-Session-Id response header on initialize.
//
// The chorus-hooks MCP client (mcp_client.rs:65-68) requires the
// Mcp-Session-Id response header per the MCP HTTP+SSE spec and errors
// `mcp initialize: no session id header` when absent. The chorus-mcp
// transport stays stateless per #2949 — we synthesize the header in the
// /mcp handler so spec-conformant clients work, without reintroducing the
// "Server not initialized" failure class.
//
// What this test covers:
//   - POST /mcp with a valid initialize JSON-RPC body
//   - Assert response status is 200
//   - Assert Mcp-Session-Id (case-insensitive) is present
//   - Assert the value is a non-empty UUID-shaped string

import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import express from 'express';
import type { Server } from 'http';
import { mountMcpEndpoint } from '../src/transport';

test('POST /mcp initialize returns Mcp-Session-Id header (#3008)', async () => {
  // Point pulse URL at a closed port — without this, the notifyTransportError
  // path could fire real nudges to silas on any non-2xx response.
  const origPulse = process.env.CHORUS_PULSE_URL;
  process.env.CHORUS_PULSE_URL = 'http://localhost:1/api/nudge';

  // Wire mountMcpEndpoint onto a fresh Express app on a random port. This is
  // the real Express + transport composition the daemon runs — the bug lives
  // in the response-header path, which only manifests against the actual
  // SDK + Express stack, not against an in-memory transport.
  const app = express();
  app.use(express.json());
  mountMcpEndpoint(app);

  const server: Server = await new Promise((resolve) => {
    const s = app.listen(0, '127.0.0.1', () => resolve(s));
  });
  const addr = server.address();
  if (!addr || typeof addr === 'string') {
    throw new Error('listen failed: no address');
  }
  const baseUrl = `http://127.0.0.1:${addr.port}`;

  try {
    const resp = await fetch(`${baseUrl}/mcp`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Accept: 'application/json, text/event-stream',
        'X-Chorus-Role': 'wren',
      },
      body: JSON.stringify({
        jsonrpc: '2.0',
        id: 1,
        method: 'initialize',
        params: {
          protocolVersion: '2025-06-18',
          capabilities: {},
          clientInfo: { name: 'session-id-header-test', version: '1.0' },
        },
      }),
    });
    assert.equal(resp.status, 200, 'initialize should return 200');
    // Header lookup is case-insensitive per HTTP; fetch's Headers normalizes.
    const sessionId = resp.headers.get('mcp-session-id');
    assert.ok(
      sessionId !== null && sessionId !== '',
      `expected non-empty Mcp-Session-Id header on initialize, got: ${sessionId}`,
    );
    // UUID v4 shape — 36 chars with hyphens at the standard positions.
    assert.match(
      sessionId!,
      /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i,
      `Mcp-Session-Id should be UUID-shaped, got: ${sessionId}`,
    );
    // Drain the SSE body so the connection closes cleanly.
    await resp.text();
  } finally {
    await new Promise<void>((resolve) => server.close(() => resolve()));
    if (origPulse === undefined) delete process.env.CHORUS_PULSE_URL;
    else process.env.CHORUS_PULSE_URL = origPulse;
  }
});
