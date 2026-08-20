// @test-type: integration — in-process MCP server + stubbed fetch; hermetic (no live pulse)
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

// #3931 (2026-08-20) — INVERTED from its #3001 original. The old assertion
// ("unknown tool fires a nudge to silas") pinned exactly the behavior #3904
// retired: caller-side refusals (Unknown tool:, Invalid arguments, refused:)
// no longer nudge ops — they caused 03:10 nudge storms three nights running.
// The positive wire-path (systemic error → POST) is covered by the
// shouldNotifyOps units + dispatch-error-integration; THIS test is now the
// integration-level negative proof that a caller-side refusal stays silent.
test('integration: caller-side unknown-tool error does NOT nudge ops (#3904)', async () => {
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
    assert.equal(
      nudgePosts.length, 0,
      `caller-side refusal must stay silent (#3904); got: ${JSON.stringify(nudgePosts.map((r) => r.body))}`,
    );
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
