// #3000 — integration test for the actual dispatch error-capture path.
// node:test runner (Node 20+ built-in). Imports buildMcpServer + the MCP
// SDK's in-process transport. No HTTP, no side-port, no Loki. Proves the
// wrap fires emit on tool throw / invalid args — same code paths the live
// daemon runs.

import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { tmpdir } from 'node:os';
import { writeFileSync, readFileSync, existsSync, unlinkSync, mkdtempSync } from 'node:fs';
import { join } from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';
import { buildMcpServer } from '../src/server';

function setupCapture(): { logPath: string; cleanup: () => void } {
  // server.ts appendChorusLog writes to CHORUS_LOG_FILE (default
  // ~/.chorus/chorus.log). Override the env to a temp file the test reads.
  const dir = mkdtempSync(join(tmpdir(), 'mcp-3000-integration-'));
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

function readEvents(logPath: string): string[] {
  if (!existsSync(logPath)) return [];
  return readFileSync(logPath, 'utf-8').trim().split('\n').filter(Boolean);
}

test('integration: unknown-tool throws → buildMcpServer emits mcp.tool.error', async () => {
  const { logPath, cleanup } = setupCapture();
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
    assert.equal(threw, true, 'expected unknown tool call to throw to caller');
    await sleep(200);
    const events = readEvents(logPath);
    const errorEvents = events.filter((e) => e.includes('mcp.tool.error'));
    assert.ok(
      errorEvents.length >= 1,
      `expected mcp.tool.error event, got events: ${JSON.stringify(events)}`,
    );
    assert.match(errorEvents[0], /"tool":"chorus_nonexistent_tool"/);
    assert.match(errorEvents[0], /"error_type":"throw"/);
  } finally {
    cleanup();
  }
});

test('integration: invalid args → buildMcpServer emits mcp.tool.error', async () => {
  const { logPath, cleanup } = setupCapture();
  try {
    const server = buildMcpServer(() => 'silas');
    const [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();
    await server.connect(serverTransport);
    const client = new Client({ name: 'test', version: '0.1.0' }, { capabilities: {} });
    await client.connect(clientTransport);

    let threw = false;
    try {
      await client.callTool({ name: 'chorus_nudge_message', arguments: {} });
    } catch {
      threw = true;
    }
    assert.equal(threw, true, 'expected invalid args to throw');
    await sleep(200);
    const events = readEvents(logPath);
    const errorEvents = events.filter((e) => e.includes('mcp.tool.error'));
    assert.ok(
      errorEvents.length >= 1,
      `expected mcp.tool.error event, got events: ${JSON.stringify(events)}`,
    );
    assert.match(errorEvents[0], /"tool":"chorus_nudge_message"/);
  } finally {
    cleanup();
  }
});
