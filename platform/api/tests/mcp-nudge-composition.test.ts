/**
 * #2820 — DEC-107 composition gate (replacement for retired
 * `nudge_force_source_gate.rs` from #2814).
 *
 * Invariant: when chorus_nudge_message is invoked, BOTH ends of the substrate
 * — sender-side `nudge.requested` (chorus-api MCP) and worker-side
 * `nudge.surfaced` (pulse delivery worker) — write to chorus.log with the
 * SAME trace_id. A future change that makes either side passive
 * (silently drops persist; silently drops delivery) breaks this assertion.
 *
 * Pre-#2804 this invariant lived as a source-grep on the bash `nudge` script.
 * That source is gone. The invariant survived; this test is its new home.
 *
 * Threat model: a refactor that removes/renames the spine event on either
 * side, or breaks trace_id propagation through pulse → worker. Both have
 * happened in this codebase before (#2764 cutover: pulse emitted with
 * `ts:` not `timestamp:`; #2727 chain: trace_id wasn't propagated to worker).
 */
import { promises as fs } from 'fs';
import os from 'os';
import path from 'path';
import express from 'express';
import { Server } from 'http';
import { buildMcpServer } from '../src/mcp/server';
import { createApp } from '../../pulse/src/service';
import { MessageStore } from '../../pulse/src/store';
import { DeliveryWorker } from '../../pulse/src/delivery-worker';
import type { InjectResult } from '../../pulse/src/delivery-worker';

describe('#2820 DEC-107 composition gate — nudge.requested + nudge.surfaced share trace_id', () => {
  let logPath: string;
  let pulseServer: Server;
  let pulsePort: number;
  let store: MessageStore;
  let worker: DeliveryWorker;

  beforeAll(async () => {
    // Hermetic chorus.log file — both writers (MCP server + pulse worker)
    // point at this single file so the test can assert ordering across them.
    const tmpDir = await fs.mkdtemp(path.join(os.tmpdir(), 'mcp-composition-'));
    logPath = path.join(tmpDir, 'chorus.log');
    process.env.CHORUS_LOG_FILE = logPath; // chorus-api MCP server
    process.env.CHORUS_LOG = logPath;       // pulse service
    process.env.PULSE_ALLOW_DIRECT_POST = '1'; // not testing the gate here
    // CHORUS_PULSE_URL is set in the test body once we know the bound port —
    // buildMcpServer reads it lazily via the env, NOT via deps.

    // In-process pulse: real store + worker, mock chorus-inject.
    const dbPath = path.join(tmpDir, 'messages.db');
    store = new MessageStore(dbPath);
    const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
    const mockInject = async (): Promise<InjectResult> => {
      // eslint-disable-next-line no-console
      console.log('[test] mockInject called');
      return { rc: 0, stderr: '' };
    };
    const emitSpine = async (event: string, fields: Record<string, unknown>) => {
      // eslint-disable-next-line no-console
      console.log('[test] emitSpine', event, JSON.stringify(fields).slice(0, 100));
      events.push({ event, fields });
      // Mirror what the real pulse emit does: write to chorus.log.
      const line = JSON.stringify({ timestamp: new Date().toISOString(), event, role: 'pulse', ...fields }) + '\n';
      await fs.appendFile(logPath, line);
    };
    worker = new DeliveryWorker(store, mockInject, emitSpine, [10, 20], async () => {});

    const app = createApp(store, worker);
    await new Promise<void>(resolve => {
      pulseServer = app.listen(0, () => {
        pulsePort = (pulseServer.address() as { port: number }).port;
        resolve();
      });
    });
    // Critical: redirect MCP's fetch to our in-process pulse, not the live
    // :3475 daemon. buildMcpServer reads this env at handler call time.
    process.env.CHORUS_PULSE_URL = `http://127.0.0.1:${pulsePort}/api/nudge`;
  });

  afterAll(async () => {
    delete process.env.CHORUS_LOG_FILE;
    delete process.env.CHORUS_LOG;
    delete process.env.PULSE_ALLOW_DIRECT_POST;
    delete process.env.CHORUS_PULSE_URL;
    store?.close();
    await new Promise<void>(resolve => pulseServer?.close(() => resolve()));
  });

  test('full chain MCP → pulse → worker writes nudge.requested + nudge.surfaced for same trace_id (DEC-107)', async () => {
    // Drive MCP handler against the live in-process pulse via fetchImpl.
    // *** LOAD-BEARING: do not remove without replacement. ***
    //
    // This safety guard caught a real bug during #2820 development: the test
    // initially relied on a `pulseUrl` deps option to redirect MCP fetches at
    // the in-process pulse fixture. `buildMcpServer` reads `CHORUS_PULSE_URL`
    // from process.env at handler call time — the deps option was silently
    // ignored. Without this guard, the test fired real `chorus_nudge_message`
    // calls into the live :3475 pulse daemon, which delivered real osascript
    // nudges to Jeff's actual terminal session. Twice. (silas gate:ops 2026-05-08)
    //
    // If you refactor pulse-URL plumbing, KEEP this assertion — it's the only
    // thing preventing a test against in-process fixture from accidentally
    // routing through the daemon if env-vs-deps semantics shift. The cost of
    // the guard is one line; the cost of removing it is real-world side
    // effects visible to a human.
    if (!process.env.CHORUS_PULSE_URL?.includes(`:${pulsePort}/`)) {
      throw new Error(`SAFETY: CHORUS_PULSE_URL must point to test port :${pulsePort}, got ${process.env.CHORUS_PULSE_URL}`);
    }

    const server = buildMcpServer(() => 'silas', {
      // CHORUS_PULSE_URL set in beforeAll redirects fetch to in-process pulse.
      fetchImpl: globalThis.fetch as never,
    });
    // @ts-expect-error - private handler access for unit test
    const handler = (server as any)._requestHandlers.get('tools/call');
    const result = await handler(
      { method: 'tools/call', params: { name: 'chorus_nudge_message', arguments: { to: 'wren', message: 'composition-test' } } },
      {},
    );
    // MCP responds with the trace.
    const traceMatch = (result.content[0].text as string).match(/trace=([0-9a-f-]+)/);
    expect(traceMatch).not.toBeNull();
    const trace = traceMatch![1];

    // Drain worker — POST /api/nudge wires worker.enqueue fire-and-forget.
    // 500ms is plenty for in-process delivery + appendFile flush.
    await new Promise(r => setTimeout(r, 500));

    // Read chorus.log and assert composition contract.
    const logContent = await fs.readFile(logPath, 'utf8');
    const lines = logContent.trim().split('\n').filter(l => l).map(l => JSON.parse(l));
    const requestedLines = lines.filter(l => l.event === 'nudge.requested' && (l.payload?.includes(trace) || l.trace_id === trace));
    const surfacedLines = lines.filter(l => l.event === 'nudge.surfaced' && l.trace_id === trace);

    if (requestedLines.length === 0 || surfacedLines.length === 0) {
      // Failure message names DEC-107 explicitly so a future debugger doesn't
      // waste time chasing the gate before reading the principle.
      throw new Error(
        `DEC-107 composition gate FAILED: persist-and-deliver invariant broken. ` +
          `nudge.requested=${requestedLines.length}, nudge.surfaced=${surfacedLines.length} for trace=${trace}. ` +
          `Both ends of the substrate must emit for the same trace_id. ` +
          `Lines: ${JSON.stringify(lines.slice(-5))}`,
      );
    }
    expect(requestedLines.length).toBeGreaterThanOrEqual(1);
    expect(surfacedLines.length).toBeGreaterThanOrEqual(1);
  });
});
