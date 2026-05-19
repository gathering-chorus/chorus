// #3001 — integration test for the chorus-mcp → silas push path.
// node:test runner (Node 20+). Replaces global fetch with a capture stub,
// triggers an MCP error via in-process MCP, asserts the stub received a
// POST to pulseUrl with `to: "silas"` and the error fields in body.

import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { tmpdir } from 'node:os';
import { mkdtempSync, existsSync, unlinkSync } from 'node:fs';
import { join } from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';
import { buildMcpServer } from '../src/server';

interface CapturedRequest {
  url: string;
  body: unknown;
}

function setupFetchCapture(): { captured: CapturedRequest[]; restore: () => void } {
  const captured: CapturedRequest[] = [];
  const origFetch = globalThis.fetch;
  globalThis.fetch = (async (input: string | URL | Request, init?: RequestInit) => {
    const url = typeof input === 'string' ? input : input.toString();
    let body: unknown = null;
    if (init?.body) {
      try {
        body = JSON.parse(String(init.body));
      } catch {
        body = String(init.body);
      }
    }
    captured.push({ url, body });
    return new Response('{"ok":true}', { status: 200, headers: { 'Content-Type': 'application/json' } });
  }) as typeof fetch;
  return {
    captured,
    restore: () => {
      globalThis.fetch = origFetch;
    },
  };
}

function setupChorusLog(): { logPath: string; cleanup: () => void } {
  const dir = mkdtempSync(join(tmpdir(), 'mcp-3001-silas-notify-'));
  const logPath = join(dir, 'chorus.log');
  const origEnv = process.env.CHORUS_LOG_FILE;
  process.env.CHORUS_LOG_FILE = logPath;
  return {
    logPath,
    cleanup: () => {
      if (origEnv === undefined) delete process.env.CHORUS_LOG_FILE;
      else process.env.CHORUS_LOG_FILE = origEnv;
      if (existsSync(logPath)) unlinkSync(logPath);
    },
  };
}

test('integration: mcp.tool.error fires nudge POST to silas via pulse', async () => {
  const fetchCapture = setupFetchCapture();
  const logCapture = setupChorusLog();
  try {
    const server = buildMcpServer(() => 'silas');
    const [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();
    await server.connect(serverTransport);
    const client = new Client({ name: 'test', version: '0.1.0' }, { capabilities: {} });
    await client.connect(clientTransport);

    let threw = false;
    try {
      await client.callTool({ name: 'chorus_nonexistent_tool', arguments: {} });
    } catch {
      threw = true;
    }
    assert.equal(threw, true);
    await sleep(250);

    const nudgePosts = fetchCapture.captured.filter((r) => r.url.includes('/api/nudge'));
    assert.ok(
      nudgePosts.length >= 1,
      `expected at least one /api/nudge POST, got ${fetchCapture.captured.length} total: ${JSON.stringify(fetchCapture.captured.map((r) => r.url))}`,
    );
    const body = nudgePosts[0].body as { from: string; to: string; content: string };
    assert.equal(body.to, 'silas', 'nudge must route to silas (ops role)');
    assert.equal(body.from, 'chorus-mcp');
    assert.match(body.content, /\[mcp\.error\]/);
    assert.match(body.content, /chorus_nonexistent_tool/);
  } finally {
    fetchCapture.restore();
    logCapture.cleanup();
  }
});

test('integration: nudge POST body NEVER routes to jeff', async () => {
  const fetchCapture = setupFetchCapture();
  const logCapture = setupChorusLog();
  try {
    const server = buildMcpServer(() => 'silas');
    const [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();
    await server.connect(serverTransport);
    const client = new Client({ name: 'test', version: '0.1.0' }, { capabilities: {} });
    await client.connect(clientTransport);

    try {
      await client.callTool({ name: 'chorus_nonexistent_tool', arguments: {} });
    } catch {
      // expected
    }
    await sleep(250);

    const nudgePosts = fetchCapture.captured.filter((r) => r.url.includes('/api/nudge'));
    for (const post of nudgePosts) {
      const body = post.body as { to: string };
      assert.notEqual(body.to, 'jeff', 'MCP error nudges must never go to jeff');
    }
  } finally {
    fetchCapture.restore();
    logCapture.cleanup();
  }
});
