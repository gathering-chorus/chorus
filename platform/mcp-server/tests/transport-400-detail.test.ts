// @test-type: integration — in-process express + real mountMcpEndpoint over a loopback socket; hermetic (fake chorus-log on PATH, no live spine)
// #3958 — the mcp.transport.error event must carry enough detail to name the
// caller: user_agent + a bounded body snippet. Negative proof per #3734: a
// violating request (valid JSON, invalid JSON-RPC → SDK 400) MUST produce an
// enriched event; a fixture where the event is missing or bare fails this test.
//
// Same harness pattern as error-capture.test.ts: fake chorus-log on PATH
// captures spine emits to a temp file.

import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { tmpdir } from 'node:os';
import { writeFileSync, readFileSync, existsSync, mkdtempSync } from 'node:fs';
import { join } from 'node:path';
import express from 'express';
import type { AddressInfo } from 'node:net';
import { mountMcpEndpoint, safeBodySnippet } from '../src/transport';

function setupCapture(): { logPath: string; restore: () => void } {
  const dir = mkdtempSync(join(tmpdir(), 'mcp-transport-400-'));
  const logPath = join(dir, 'spine.log');
  const fakeBin = join(dir, 'chorus-log');
  writeFileSync(fakeBin, `#!/bin/bash\necho "$@" >> "${logPath}"\n`, { mode: 0o755 });
  const origPath = process.env.PATH;
  process.env.PATH = `${dir}:${origPath}`;
  return { logPath, restore: () => { process.env.PATH = origPath; } };
}

async function waitForEvent(logPath: string, timeoutMs = 3000): Promise<string> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (existsSync(logPath)) {
      const content = readFileSync(logPath, 'utf-8');
      if (content.includes('mcp.transport.error')) return content;
    }
    await new Promise((r) => setTimeout(r, 50));
  }
  return existsSync(logPath) ? readFileSync(logPath, 'utf-8') : '';
}

test('SDK 400 on invalid JSON-RPC emits enriched transport.error (negative proof)', async () => {
  const { logPath, restore } = setupCapture();
  const app = express();
  app.use(express.json({ limit: '10mb' }));
  mountMcpEndpoint(app);
  const srv = app.listen(0, '127.0.0.1');
  await new Promise((r) => srv.once('listening', r));
  const port = (srv.address() as AddressInfo).port;
  try {
    const resp = await fetch(`http://127.0.0.1:${port}/mcp`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Accept: 'application/json, text/event-stream',
        'X-Chorus-Role': 'silas',
        'User-Agent': 'transport-400-test/1.0',
      },
      body: JSON.stringify({ foo: 1 }),
    });
    assert.equal(resp.status, 400, 'invalid JSON-RPC must 400');
    const captured = await waitForEvent(logPath);
    assert.ok(
      captured.includes('mcp.transport.error'),
      `400 through the handler must emit a transport.error event; captured: ${JSON.stringify(captured)}`,
    );
    assert.ok(captured.includes('user_agent=transport-400-test/1.0'), `event must carry user_agent; captured: ${captured}`);
    assert.ok(captured.includes('"foo":1'), `event must carry the body snippet; captured: ${captured}`);
  } finally {
    srv.close();
    restore();
  }
});

test('safeBodySnippet bounds and never throws', () => {
  assert.equal(safeBodySnippet(undefined), '');
  assert.equal(safeBodySnippet({ a: 1 }), '{"a":1}');
  assert.equal(safeBodySnippet('x'.repeat(1000)).length, 300);
  const cyc: Record<string, unknown> = {};
  cyc['self'] = cyc;
  assert.equal(safeBodySnippet(cyc), '[unserializable]');
});
