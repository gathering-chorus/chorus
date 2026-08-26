/**
 * Phase A0 launch safety: filesystem serialization across daemon processes and
 * durable reservation-before-spawn for both present and GO paths.
 * @test-type: unit — temp dirs, captured spawn, and two hermetic node children
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync,
  utimesSync, writeFileSync,
} from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawn as spawnProcess } from 'node:child_process';
import { pathToFileURL } from 'node:url';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';
import { buildMcpServer, type SpawnFn } from '../src/server';
import { readRun, writeRun, withRunLaunchLock } from '../src/werk-run-store';

type SpawnCall = { command: string; args: string[] };

async function withServer(
  runsDir: string,
  spawnFn: SpawnFn,
  role: 'kade' | 'wren',
  fn: (client: Client) => Promise<void>,
): Promise<void> {
  const server = buildMcpServer(() => role, { spawnFn, runsDir, cardsPath: '/fake/cards' });
  const [ct, st] = InMemoryTransport.createLinkedPair();
  const client = new Client({ name: 'werk-launch-safety', version: '1.0' });
  await Promise.all([server.connect(st), client.connect(ct)]);
  try { await fn(client); } finally { await client.close(); await server.close(); }
}

/** Two independent MCP Server instances model two daemon callers while sharing
 * only the filesystem run directory and the injected spawn counter. */
async function withTwoServers(
  runsDir: string,
  spawnFn: SpawnFn,
  role: 'kade' | 'wren',
  fn: (first: Client, second: Client) => Promise<void>,
): Promise<void> {
  const firstServer = buildMcpServer(() => role, { spawnFn, runsDir, cardsPath: '/fake/cards' });
  const secondServer = buildMcpServer(() => role, { spawnFn, runsDir, cardsPath: '/fake/cards' });
  const [firstClientTransport, firstServerTransport] = InMemoryTransport.createLinkedPair();
  const [secondClientTransport, secondServerTransport] = InMemoryTransport.createLinkedPair();
  const firstClient = new Client({ name: 'werk-launch-safety-first', version: '1.0' });
  const secondClient = new Client({ name: 'werk-launch-safety-second', version: '1.0' });
  await Promise.all([
    firstServer.connect(firstServerTransport), firstClient.connect(firstClientTransport),
    secondServer.connect(secondServerTransport), secondClient.connect(secondClientTransport),
  ]);
  try {
    await fn(firstClient, secondClient);
  } finally {
    await Promise.all([
      firstClient.close(), secondClient.close(), firstServer.close(), secondServer.close(),
    ]);
  }
}

function reservationProbe(
  runsDir: string,
  card: number,
  expectedGo: boolean,
  sink: SpawnCall[],
): SpawnFn {
  return ((command: string, args: string[]) => {
    const atSpawn = readRun(card, runsDir);
    assert.ok(atSpawn, 'a durable run reservation exists before spawn');
    assert.equal(atSpawn.phase, 'running');
    assert.equal(atSpawn.go, expectedGo);
    assert.equal(atSpawn.pid, undefined, 'PID is added only after spawn returns');
    sink.push({ command, args });
    return { pid: process.pid, unref() {} };
  }) as unknown as SpawnFn;
}

/** Real child_process.spawn returns first and reports ENOENT on `error`. */
function asyncEnoentSpawn(): SpawnFn {
  return (() => spawnProcess(
    '/definitely-not-present/chorus-a0-act',
    [],
    { stdio: 'ignore' },
  ) as unknown as ReturnType<SpawnFn>) as SpawnFn;
}

async function waitUntil(predicate: () => boolean, timeoutMs = 5_000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (!predicate()) {
    if (Date.now() >= deadline) throw new Error(`timed out after ${timeoutMs}ms`);
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
}

function childCompletion(child: ReturnType<typeof spawnProcess>): Promise<void> {
  let stderr = '';
  child.stderr?.on('data', (chunk) => { stderr += String(chunk); });
  return new Promise((resolve, reject) => {
    child.once('error', reject);
    child.once('exit', (code, signal) => {
      if (code === 0) resolve();
      else reject(new Error(`lock child exited code=${code} signal=${signal}: ${stderr}`));
    });
  });
}

test('per-card launch lock serializes critical sections across separate node processes', async () => {
  const temp = mkdtempSync(path.join(os.tmpdir(), 'werk-launch-lock-processes-'));
  const runsDir = path.join(temp, 'runs');
  const readyFile = path.join(temp, 'ready');
  const startFile = path.join(temp, 'start');
  const eventsFile = path.join(temp, 'events');
  const mcpDir = path.resolve(__dirname, '..');
  const moduleUrl = pathToFileURL(path.join(mcpDir, 'src', 'werk-run-store.ts')).href;
  const childCode = `
    import { appendFileSync, existsSync } from 'node:fs';
    const loaded = await import(${JSON.stringify(moduleUrl)});
    const withRunLaunchLock = loaded.withRunLaunchLock
      ?? loaded.default?.withRunLaunchLock
      ?? loaded['module.exports']?.withRunLaunchLock;
    if (typeof withRunLaunchLock !== 'function') {
      throw new Error('werk-run-store import did not expose withRunLaunchLock');
    }
    appendFileSync(${JSON.stringify(readyFile)}, process.pid + '\\n');
    while (!existsSync(${JSON.stringify(startFile)})) {
      await new Promise((resolve) => setTimeout(resolve, 5));
    }
    await withRunLaunchLock(5100, async () => {
      appendFileSync(${JSON.stringify(eventsFile)}, 'enter ' + process.pid + '\\n');
      await new Promise((resolve) => setTimeout(resolve, 150));
      appendFileSync(${JSON.stringify(eventsFile)}, 'exit ' + process.pid + '\\n');
    }, ${JSON.stringify(runsDir)}, { timeoutMs: 5_000, pollMs: 5 });
  `;
  const spawnChild = () => spawnProcess(
    process.execPath,
    ['--import', 'tsx', '--input-type=module', '--eval', childCode],
    { cwd: mcpDir, stdio: ['ignore', 'ignore', 'pipe'] },
  );

  try {
    const first = spawnChild();
    const second = spawnChild();
    await waitUntil(() => {
      if (!existsSync(readyFile)) return false;
      return readFileSync(readyFile, 'utf8').trim().split('\n').filter(Boolean).length === 2;
    });
    writeFileSync(startFile, 'go');
    await Promise.all([childCompletion(first), childCompletion(second)]);

    const events = readFileSync(eventsFile, 'utf8').trim().split('\n');
    let active = 0;
    for (const event of events) {
      if (event.startsWith('enter ')) active += 1;
      if (event.startsWith('exit ')) active -= 1;
      assert.ok(active >= 0 && active <= 1, `critical sections overlapped: ${events.join(', ')}`);
    }
    assert.equal(active, 0);
    assert.equal(events.filter((line) => line.startsWith('enter ')).length, 2);
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
});

test('launch lock reclaims a dead owner immediately', async () => {
  const runsDir = mkdtempSync(path.join(os.tmpdir(), 'werk-launch-dead-lock-'));
  const card = 5105;
  writeFileSync(path.join(runsDir, `${card}.launch.lock`), JSON.stringify({
    token: 'dead-owner', pid: 999999, acquiredAt: new Date().toISOString(),
  }));
  let entered = false;
  try {
    await withRunLaunchLock(card, () => { entered = true; }, runsDir, { timeoutMs: 250, pollMs: 5 });
    assert.equal(entered, true);
  } finally {
    rmSync(runsDir, { recursive: true, force: true });
  }
});

test('launch lock never steals from a live owner merely because it is old', async () => {
  const runsDir = mkdtempSync(path.join(os.tmpdir(), 'werk-launch-live-lock-'));
  const card = 5106;
  writeFileSync(path.join(runsDir, `${card}.launch.lock`), JSON.stringify({
    token: 'live-owner', pid: process.pid, acquiredAt: '2000-01-01T00:00:00.000Z',
  }));
  try {
    await assert.rejects(
      withRunLaunchLock(card, () => undefined, runsDir, { timeoutMs: 50, pollMs: 5 }),
      /timed out waiting/,
    );
  } finally {
    rmSync(runsDir, { recursive: true, force: true });
  }
});

test('launch lock is complete at first fixed-path visibility', async () => {
  const runsDir = mkdtempSync(path.join(os.tmpdir(), 'werk-launch-complete-lock-'));
  const card = 5111;
  try {
    await withRunLaunchLock(card, () => {
      const owner = JSON.parse(readFileSync(path.join(runsDir, `${card}.launch.lock`), 'utf8')) as {
        token?: unknown; pid?: unknown; acquiredAt?: unknown;
      };
      assert.equal(owner.pid, process.pid);
      assert.equal(typeof owner.token, 'string');
      assert.equal(typeof owner.acquiredAt, 'string');
    }, runsDir);
  } finally {
    rmSync(runsDir, { recursive: true, force: true });
  }
});

test('malformed ownerless lock is never stolen merely because it is old', async () => {
  const runsDir = mkdtempSync(path.join(os.tmpdir(), 'werk-launch-ownerless-lock-'));
  const card = 5112;
  const lockFile = path.join(runsDir, `${card}.launch.lock`);
  writeFileSync(lockFile, '');
  utimesSync(lockFile, new Date(0), new Date(0));
  try {
    await assert.rejects(
      withRunLaunchLock(card, () => undefined, runsDir, { timeoutMs: 50, pollMs: 5 }),
      /timed out waiting/,
    );
  } finally {
    rmSync(runsDir, { recursive: true, force: true });
  }
});

test('present launch persists a pid-less reservation before spawn, then records the PID', async () => {
  const runsDir = mkdtempSync(path.join(os.tmpdir(), 'werk-present-reservation-'));
  const sink: SpawnCall[] = [];
  try {
    await withServer(runsDir, reservationProbe(runsDir, 5101, false, sink), 'kade', async (client) => {
      await client.callTool({ name: 'chorus_werk', arguments: { role: 'kade', card_id: 5101 } });
    });
    assert.equal(sink.length, 1);
    assert.equal(readRun(5101, runsDir)?.pid, process.pid);
  } finally {
    rmSync(runsDir, { recursive: true, force: true });
  }
});

test('concurrent present callers across independent MCP servers spawn only one act', async () => {
  const runsDir = mkdtempSync(path.join(os.tmpdir(), 'werk-present-concurrent-'));
  const sink: SpawnCall[] = [];
  try {
    await withTwoServers(runsDir, reservationProbe(runsDir, 5107, false, sink), 'kade', async (first, second) => {
      await Promise.all([
        first.callTool({ name: 'chorus_werk', arguments: { role: 'kade', card_id: 5107 } }),
        second.callTool({ name: 'chorus_werk', arguments: { role: 'kade', card_id: 5107 } }),
      ]);
    });
    assert.equal(sink.length, 1, 'the second caller must decide against the first durable run record');
    assert.equal(readRun(5107, runsDir)?.pid, process.pid);
  } finally {
    rmSync(runsDir, { recursive: true, force: true });
  }
});

test('GO launch persists a pid-less reservation before spawn, then records the PID', async () => {
  const runsDir = mkdtempSync(path.join(os.tmpdir(), 'werk-go-reservation-'));
  const werkBase = mkdtempSync(path.join(os.tmpdir(), 'werk-go-base-'));
  const originalBase = process.env.CHORUS_WERK_BASE;
  const sink: SpawnCall[] = [];
  mkdirSync(path.join(werkBase, 'wren-5102'));
  writeRun({
    runId: 'presented-round', card: 5102, role: 'wren', go: false,
    phase: 'presented', startedAt: new Date().toISOString(), patchId: 'p1',
  }, runsDir);
  process.env.CHORUS_WERK_BASE = werkBase;
  try {
    await withServer(runsDir, reservationProbe(runsDir, 5102, true, sink), 'wren', async (client) => {
      await client.callTool({ name: 'chorus_werk', arguments: { role: 'wren', card_id: 5102, go: true } });
    });
    assert.equal(sink.length, 1);
    assert.equal(readRun(5102, runsDir)?.pid, process.pid);
    assert.equal(readRun(5102, runsDir)?.go, true);
  } finally {
    if (originalBase === undefined) delete process.env.CHORUS_WERK_BASE;
    else process.env.CHORUS_WERK_BASE = originalBase;
    rmSync(runsDir, { recursive: true, force: true });
    rmSync(werkBase, { recursive: true, force: true });
  }
});

test('concurrent GO callers across independent MCP servers spawn only one land act', async () => {
  const runsDir = mkdtempSync(path.join(os.tmpdir(), 'werk-go-concurrent-'));
  const werkBase = mkdtempSync(path.join(os.tmpdir(), 'werk-go-concurrent-base-'));
  const originalBase = process.env.CHORUS_WERK_BASE;
  const sink: SpawnCall[] = [];
  mkdirSync(path.join(werkBase, 'wren-5108'));
  writeRun({
    runId: 'presented-round', card: 5108, role: 'wren', go: false,
    phase: 'presented', startedAt: new Date().toISOString(), patchId: 'p1',
  }, runsDir);
  process.env.CHORUS_WERK_BASE = werkBase;
  try {
    await withTwoServers(runsDir, reservationProbe(runsDir, 5108, true, sink), 'wren', async (first, second) => {
      await Promise.all([
        first.callTool({ name: 'chorus_werk', arguments: { role: 'wren', card_id: 5108, go: true } }),
        second.callTool({ name: 'chorus_werk', arguments: { role: 'wren', card_id: 5108, go: true } }),
      ]);
    });
    assert.equal(sink.length, 1, 'the second GO must observe the first live land reservation');
    assert.equal(readRun(5108, runsDir)?.pid, process.pid);
    assert.equal(readRun(5108, runsDir)?.go, true);
  } finally {
    if (originalBase === undefined) delete process.env.CHORUS_WERK_BASE;
    else process.env.CHORUS_WERK_BASE = originalBase;
    rmSync(runsDir, { recursive: true, force: true });
    rmSync(werkBase, { recursive: true, force: true });
  }
});

test('present handles a real asynchronous spawn ENOENT without returning launched or crashing', async () => {
  const runsDir = mkdtempSync(path.join(os.tmpdir(), 'werk-present-async-spawn-error-'));
  try {
    await withServer(runsDir, asyncEnoentSpawn(), 'kade', async (client) => {
      await assert.rejects(
        client.callTool({ name: 'chorus_werk', arguments: { role: 'kade', card_id: 5113 } }),
        /ENOENT/,
      );
    });
    const failed = readRun(5113, runsDir);
    assert.equal(failed?.phase, 'failed');
    assert.match(failed?.failureReason ?? '', /ENOENT/);
  } finally {
    rmSync(runsDir, { recursive: true, force: true });
  }
});

test('GO handles a real asynchronous spawn ENOENT and persists failed state', async () => {
  const runsDir = mkdtempSync(path.join(os.tmpdir(), 'werk-go-async-spawn-error-'));
  const werkBase = mkdtempSync(path.join(os.tmpdir(), 'werk-go-async-spawn-base-'));
  const originalBase = process.env.CHORUS_WERK_BASE;
  mkdirSync(path.join(werkBase, 'wren-5114'));
  writeRun({
    runId: 'presented-round', card: 5114, role: 'wren', go: false,
    phase: 'presented', startedAt: new Date().toISOString(), patchId: 'p1',
  }, runsDir);
  process.env.CHORUS_WERK_BASE = werkBase;
  try {
    await withServer(runsDir, asyncEnoentSpawn(), 'wren', async (client) => {
      await assert.rejects(
        client.callTool({ name: 'chorus_werk', arguments: { role: 'wren', card_id: 5114, go: true } }),
        /ENOENT/,
      );
    });
    const failed = readRun(5114, runsDir);
    assert.equal(failed?.phase, 'failed');
    assert.equal(failed?.go, true);
    assert.match(failed?.failureReason ?? '', /ENOENT/);
  } finally {
    if (originalBase === undefined) delete process.env.CHORUS_WERK_BASE;
    else process.env.CHORUS_WERK_BASE = originalBase;
    rmSync(runsDir, { recursive: true, force: true });
    rmSync(werkBase, { recursive: true, force: true });
  }
});

test('present path fails closed before spawn when the atomic reservation cannot replace state', async () => {
  const runsDir = mkdtempSync(path.join(os.tmpdir(), 'werk-present-fail-closed-'));
  const sink: SpawnCall[] = [];
  mkdirSync(path.join(runsDir, '5103.json'));
  const spawnFn = ((command: string, args: string[]) => {
    sink.push({ command, args });
    return { pid: process.pid, unref() {} };
  }) as unknown as SpawnFn;
  try {
    await withServer(runsDir, spawnFn, 'kade', async (client) => {
      await assert.rejects(
        client.callTool({ name: 'chorus_werk', arguments: { role: 'kade', card_id: 5103 } }),
        /atomic write failed/,
      );
    });
    assert.equal(sink.length, 0, 'act must not spawn without durable launch state');
  } finally {
    rmSync(runsDir, { recursive: true, force: true });
  }
});

test('GO path fails closed before spawn when the atomic reservation cannot replace state', async () => {
  const runsDir = mkdtempSync(path.join(os.tmpdir(), 'werk-go-fail-closed-'));
  const werkBase = mkdtempSync(path.join(os.tmpdir(), 'werk-go-fail-base-'));
  const originalBase = process.env.CHORUS_WERK_BASE;
  const sink: SpawnCall[] = [];
  mkdirSync(path.join(runsDir, '5104.json'));
  mkdirSync(path.join(werkBase, 'wren-5104'));
  process.env.CHORUS_WERK_BASE = werkBase;
  const spawnFn = ((command: string, args: string[]) => {
    sink.push({ command, args });
    return { pid: process.pid, unref() {} };
  }) as unknown as SpawnFn;
  try {
    await withServer(runsDir, spawnFn, 'wren', async (client) => {
      await assert.rejects(
        client.callTool({ name: 'chorus_werk', arguments: { role: 'wren', card_id: 5104, go: true } }),
        /atomic write failed/,
      );
    });
    assert.equal(sink.length, 0, 'land act must not spawn without durable launch state');
  } finally {
    if (originalBase === undefined) delete process.env.CHORUS_WERK_BASE;
    else process.env.CHORUS_WERK_BASE = originalBase;
    rmSync(runsDir, { recursive: true, force: true });
    rmSync(werkBase, { recursive: true, force: true });
  }
});
