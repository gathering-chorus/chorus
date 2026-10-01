// @test-type: integration — spawns its own Clearing on a test port; pulse dead-ported, nudge mocked (own world).
/**
 * Clearing UI Validation Tests — #1818
 *
 * Tests what Jeff SEES in the Clearing, not internals.
 * Every test verifies a user-visible behavior.
 */


import { execSync, spawn, ChildProcess } from 'child_process';
import * as http from 'http';
import * as path from 'path';
import { io as ioClient, Socket as ClientSocket } from 'socket.io-client';

// Ephemeral test Clearing — #2166.
// Spawn a dedicated Clearing server on a random ephemeral port in beforeAll,
// kill in afterAll. Tests POST against the test instance instead of Jeff's
// live Clearing at :3470, so marker noise doesn't land in his feed and socket
// fanout stays scoped. The mock nudge script below prevents any nudge from
// firing even if a code path tries. HERMETIC_TEST_MODE and the old
// describe.skip gate are removed.
//
// Random port per run avoids EADDRINUSE when a prior aborted run leaves a
// zombie holding a fixed port (hook blocks manual kill cleanup).
const TEST_PORT = 11000 + Math.floor(Math.random() * 8000);
const TEST_HTTPS_PORT = 21000 + Math.floor(Math.random() * 8000);
const CLEARING_URL = `http://localhost:${TEST_PORT}`;
// Relative to this test file (tests/ → ../dist/server.js) — never a machine-absolute
// path: the ubuntu CI runner roots the repo at /home/runner/work, not /Users/jeffbridwell (#3528).
const CLEARING_SERVER = path.join(__dirname, '..', 'dist', 'server.js');
let clearingProc: ChildProcess | null = null;

// Mock nudge script — writes to temp file instead of osascript injection.
// Tests verify Clearing filters, not nudge delivery (that's nudge-integration.test.ts).
const MOCK_NUDGE_DIR = '/tmp/clearing-test-nudges';
const MOCK_NUDGE_SCRIPT = '/tmp/clearing-test-mock-nudge';
import * as fs from 'fs';
import * as os from 'os';
const { testClearingEnv } = require('./lib/test-clearing-world.cjs');
const TEST_WORLD = fs.mkdtempSync(path.join(os.tmpdir(), 'clearing-ui-'));
const TEST_TOKEN = `test-${process.pid}-${Date.now()}`;

beforeAll(async () => {
  // Create mock nudge that logs but doesn't inject
  fs.mkdirSync(MOCK_NUDGE_DIR, { recursive: true });
  fs.writeFileSync(MOCK_NUDGE_SCRIPT, `#!/bin/bash
# Mock nudge — no osascript, no injection. Logs to temp file.
TARGET="$1"; shift; MSG="$*"
echo "$(date +%s) | $TARGET | $MSG" >> ${MOCK_NUDGE_DIR}/nudge.log
echo "DELIVERED to $TARGET at $(TZ=America/New_York date '+%Y-%m-%d %H:%M')"
`, { mode: 0o755 });

  // Spawn test-mode Clearing on TEST_PORT (#2166).
  clearingProc = spawn('node', [CLEARING_SERVER], {
    env: {
      ...process.env,
      // #4417 — the whole world, not just ports: HOME, projects dir, offsets,
      // journal, message file, spine log, messages.db (test-clearing-world.cjs).
      ...testClearingEnv(TEST_WORLD, TEST_PORT, TEST_TOKEN),
      CLEARING_HTTPS_PORT: String(TEST_HTTPS_PORT),
      NUDGE_BINARY: MOCK_NUDGE_SCRIPT,
    },
    stdio: 'pipe',
    detached: false,
  });
  const spawnLog: string[] = [];
  clearingProc.stderr?.on('data', (d) => spawnLog.push(`[err] ${d.toString()}`));
  clearingProc.stdout?.on('data', (d) => spawnLog.push(`[out] ${d.toString()}`));
  clearingProc.on('exit', (code, signal) => spawnLog.push(`[exit] code=${code} signal=${signal}`));

  // Wait for /health to respond 200 (up to 10s)
  const deadline = Date.now() + 10000;
  while (Date.now() < deadline) {
    try {
      const code = execSync(`curl -sf -o /dev/null -w "%{http_code}" ${CLEARING_URL}/health`, { encoding: 'utf-8', timeout: 1000 }).trim();
      if (code === '200') return;
    } catch { /* ignore */ }
    await new Promise(r => setTimeout(r, 200));
  }
  throw new Error(`test Clearing failed to start on port ${TEST_PORT} within 10s:\n${spawnLog.join('')}`);
});

afterAll(async () => {
  try { fs.rmSync(TEST_WORLD, { recursive: true, force: true }); } catch { /* ignore */ }
  try { fs.unlinkSync(MOCK_NUDGE_SCRIPT); } catch { /* ignore */ }
  try { fs.rmSync(MOCK_NUDGE_DIR, { recursive: true }); } catch { /* ignore */ }
  if (clearingProc && !clearingProc.killed) {
    clearingProc.kill('SIGTERM');
    // Give it a moment to shut down gracefully
    await new Promise(r => setTimeout(r, 300));
    if (!clearingProc.killed) clearingProc.kill('SIGKILL');
  }
});

// Track all Socket.IO clients for cleanup
const activeClients: ClientSocket[] = [];
function createClient(): ClientSocket {
  const client = ioClient(CLEARING_URL, { forceNew: true });
  activeClients.push(client);
  return client;
}

afterAll(async () => {
  for (const client of activeClients) {
    try { client.removeAllListeners(); client.disconnect(); client.close(); } catch { /* ignore */ }
  }
  // Allow event loop to drain
  await new Promise(r => setTimeout(r, 500));
});

// #4417 — waits for the room to go quiet instead of sleeping a fixed 1-6s:
// done as soon as the message list holds still across two reads 100ms apart,
// at most 3s. The feedback-loop cases used to sleep 6s for an echo that, in
// this test world (nudges mocked, no transcripts to tail), cannot arrive; the
// echo itself is proven in session-tailer-unit (expectEcho).
async function settle(): Promise<void> {
  const deadline = Date.now() + 3000;
  let last = -1;
  while (Date.now() < deadline) {
    const n = (await getMessages(500)).length;
    if (n === last) return;
    last = n;
    await new Promise(r => setTimeout(r, 100));
  }
}

// #4417 — Jeff speaks through the page's socket (a local socket is Jeff), not
// through /api/message with the shared machine credential, which is a door a
// machine should not be able to use as him. Returns 200 when the room accepted it.
async function sayAsJeff(text: string): Promise<number> {
  const c = createClient();
  await new Promise<void>((resolve, reject) => { c.on('connect', () => resolve()); c.on('connect_error', reject); });
  const ack = await new Promise<{ ok: boolean }>((resolve) => c.emit('jeff-message', { text }, resolve));
  c.disconnect();
  return ack.ok ? 200 : 500;
}

// Helper: GET JSON from Clearing API
function getMessages(limit = 10): Promise<any[]> {
  return new Promise((resolve, reject) => {
    http.get(`${CLEARING_URL}/api/messages?limit=${limit}`, (res) => {
      let data = '';
      res.on('data', (chunk) => data += chunk);
      res.on('end', () => {
        try {
          const parsed = JSON.parse(data);
          resolve(Array.isArray(parsed) ? parsed : parsed.messages || []);
        } catch {
          resolve([]);
        }
      });
    }).on('error', reject);
  });
}

// Helper: check if Clearing is running.
// Retries up to 5 times with 10s per-attempt timeout — jest workers running
// peer suites in parallel can starve the curl process momentarily, causing
// false precondition failures even when the service is healthy.
function clearingIsUp(): boolean {
  for (let i = 0; i < 5; i++) {
    try {
      const result = execSync(
        `curl -sf -o /dev/null -w "%{http_code}" ${CLEARING_URL}/health`,
        { encoding: 'utf-8', timeout: 10000 }
      );
      if (result.trim() === '200') return true;
    } catch {
      // fall through and retry
    }
  }
  return false;
}

// ═══════════════════════════════════════════════════════════════════════════
// PRECONDITION
// ═══════════════════════════════════════════════════════════════════════════

describe('Precondition: Clearing service', () => {
  test(`test Clearing is running on ephemeral port ${TEST_PORT}`, () => {
    expect(clearingIsUp()).toBe(true);
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// AC2: Role-to-role nudges do NOT appear in Clearing UI
// ═══════════════════════════════════════════════════════════════════════════

describe('AC2 (re-ruled #3862): role-to-role nudges APPEAR, typed role-to-role — Jeff 2026-08-13: no hidden nudges', () => {
  test('nudge from kade to silas appears, typed role-to-role', async () => {
    const marker = `AC2-TEST-${Date.now()}`;
    // Post a role-to-role nudge with [nudge from] prefix — must be filtered
    const body = JSON.stringify({ from: 'kade', text: `[nudge from kade] ${marker} — test nudge that should not appear in Clearing` });
    await new Promise<void>((resolve, reject) => {
      const req = http.request(`${CLEARING_URL}/api/message`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(body), ...AUTH_HEADERS },
      }, (res) => { res.on('data', () => {}); res.on('end', () => resolve()); });
      req.on('error', reject); req.write(body); req.end();
    });

    await settle();
    const messages = await getMessages(50);
    const found = messages.filter((m: any) => (m.text || '').includes(marker));
    expect(found).toHaveLength(1);
    expect(found[0].type).toBe('role-to-role');
  });

  test('nudge from wren to kade appears, typed role-to-role', async () => {
    const marker = `AC2-WK-${Date.now()}`;
    const body = JSON.stringify({ from: 'wren', text: `[nudge from wren] ${marker} — wren to kade test` });
    await new Promise<void>((resolve, reject) => {
      const req = http.request(`${CLEARING_URL}/api/message`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(body), ...AUTH_HEADERS },
      }, (res) => { res.on('data', () => {}); res.on('end', () => resolve()); });
      req.on('error', reject); req.write(body); req.end();
    });

    await settle();
    const messages = await getMessages(50);
    const found = messages.filter((m: any) => (m.text || '').includes(marker));
    expect(found).toHaveLength(1);
    expect(found[0].type).toBe('role-to-role');
  });

  test('[nudge from] prefix is typed role-to-role and stays visible', async () => {
    const marker = `AC2-PREFIX-${Date.now()}`;
    const body = JSON.stringify({ from: 'kade', text: `[nudge from kade] ${marker}` });
    await new Promise<void>((resolve, reject) => {
      const req = http.request(`${CLEARING_URL}/api/message`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(body), ...AUTH_HEADERS },
      }, (res) => { res.on('data', () => {}); res.on('end', () => resolve()); });
      req.on('error', reject); req.write(body); req.end();
    });

    await settle();
    const messages = await getMessages(50);
    const found = messages.filter((m: any) => (m.text || '').includes(marker));
    expect(found).toHaveLength(1);
    expect(found[0].type).toBe('role-to-role');
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// AC4: No feedback loop — Clearing does not re-inject its own messages
// ═══════════════════════════════════════════════════════════════════════════

// Helper: POST a message to Clearing API (top-level, used across describes)
// #3966 — the room's write door refuses anonymous posts; carry the machine's
// bridge token (the spawned server generates the file at boot if missing).
// #4417 — the test Clearing's own credential (written into its temp HOME by
// test-clearing-world.cjs); the suite never reads the live one.
const AUTH_HEADERS = { Authorization: `Bearer ${TEST_TOKEN}` };

function postMessage(from: string, text: string): Promise<number> {
  return new Promise((resolve, reject) => {
    const body = JSON.stringify({ from, text });
    const req = http.request(`${CLEARING_URL}/api/message`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(body), ...AUTH_HEADERS },
    }, (res) => {
      res.on('data', () => {});
      res.on('end', () => resolve(res.statusCode || 0));
    });
    req.on('error', reject);
    req.write(body);
    req.end();
  });
}

describe('AC4: No feedback loop — messages appear exactly once', () => {
  test('message posted via /api/message appears exactly once', async () => {
    const marker = `AC4-ECHO-${Date.now()}`;

    const status = await sayAsJeff(`${marker} — should appear exactly once`);
    expect(status).toBe(200);

    // Wait for any potential feedback loop to trigger
    await settle();

    const messages = await getMessages(100);
    const occurrences = messages.filter((m: any) => (m.text || '').includes(marker));

    expect(occurrences.length).toBe(1);
  });

  test('role response posted via /api/message appears exactly once', async () => {
    const marker = `AC4-ROLE-${Date.now()}`;

    const status = await postMessage('wren', `${marker} — role response, no echo`);
    expect(status).toBe(200);

    await settle();

    const messages = await getMessages(100);
    const occurrences = messages.filter((m: any) => (m.text || '').includes(marker));

    expect(occurrences.length).toBe(1);
  });

  test('rapid-fire messages do not multiply', async () => {
    const marker = `AC4-RAPID-${Date.now()}`;

    // Send 3 distinct messages quickly
    await sayAsJeff(`${marker}-A`);
    await sayAsJeff(`${marker}-B`);
    await sayAsJeff(`${marker}-C`);

    await settle();

    const messages = await getMessages(100);
    const countA = messages.filter((m: any) => (m.text || '').includes(`${marker}-A`)).length;
    const countB = messages.filter((m: any) => (m.text || '').includes(`${marker}-B`)).length;
    const countC = messages.filter((m: any) => (m.text || '').includes(`${marker}-C`)).length;

    expect(countA).toBe(1);
    expect(countB).toBe(1);
    expect(countC).toBe(1);
  });

  test('message posted via REST does not echo back as duplicate (no feedback loop)', async () => {
    const marker = `AC4-REST-ECHO-${Date.now()}`;

    // Post via REST — this path does NOT trigger nudge injection
    const status = await sayAsJeff(`${marker} — REST echo test`);
    expect(status).toBe(200);

    await settle();

    const messages = await getMessages(100);
    const occurrences = messages.filter((m: any) => (m.text || '').includes(marker));
    expect(occurrences.length).toBe(1);
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// AC3: Role-to-role /chat messages do NOT appear in Clearing send box
// ═══════════════════════════════════════════════════════════════════════════

describe('AC3: Role-to-role /chat messages do NOT appear in Clearing', () => {
  // #4417 — the chat.sh case is gone. It started a real chat (silas→kade) and a
  // real nudge on every nightly, and its catch passed it whatever happened.
  // The property it meant — a nudge pulse delivered is not shown as Jeff — is
  // proven in session-tailer-unit ("#4362 a nudge pulse delivered is not Jeff").

  test('[chat] prefixed messages appear, typed role-to-role (#3862 ruling)', async () => {
    const marker = `AC3-PREFIX-${Date.now()}`;

    // Post a [chat] message directly to Clearing API — verify it's filtered
    const body = JSON.stringify({ from: 'silas', text: `[chat] ${marker} — should be filtered` });
    await new Promise<void>((resolve, reject) => {
      const req = http.request(`${CLEARING_URL}/api/message`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(body), ...AUTH_HEADERS },
      }, (res) => {
        res.on('data', () => {});
        res.on('end', () => resolve());
      });
      req.on('error', reject);
      req.write(body);
      req.end();
    });

    await settle();
    const messages = await getMessages(100);
    const chatMessages = messages.filter((m: any) =>
      m.from === 'silas' && (m.text || '').includes(marker)
    );
    expect(chatMessages).toHaveLength(1);
    expect(chatMessages[0].type).toBe('role-to-role');
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// AC5: Guest identity — role names display correctly, not "Guest"
// ═══════════════════════════════════════════════════════════════════════════

describe('AC5: Guest identity displays correctly', () => {
  test('message from role via /api/message shows role name as sender', async () => {
    const marker = `AC5-ROLE-${Date.now()}`;

    const body = JSON.stringify({ from: 'wren', text: `${marker} — PM thinking` });
    await new Promise<void>((resolve, reject) => {
      const req = http.request(`${CLEARING_URL}/api/message`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(body), ...AUTH_HEADERS },
      }, (res) => {
        res.on('data', () => {});
        res.on('end', () => resolve());
      });
      req.on('error', reject);
      req.write(body);
      req.end();
    });

    await settle();

    const messages = await getMessages(50);
    const match = messages.find((m: any) => (m.text || '').includes(marker));

    expect(match).toBeDefined();
    expect(match.from).toBe('wren');
  });

  test('message from jeff shows jeff as sender', async () => {
    const marker = `AC5-JEFF-${Date.now()}`;

    await sayAsJeff(marker);
    await settle();

    const messages = await getMessages(50);
    const match = messages.find((m: any) => (m.text || '').includes(marker));

    expect(match).toBeDefined();
    expect(match.from).toBe('jeff');
  });

  test('each role name is preserved in message attribution', async () => {
    const marker = `AC5-ATTR-${Date.now()}`;

    for (const role of ['wren', 'silas', 'kade']) {
      const body = JSON.stringify({ from: role, text: `${marker}-${role}` });
      await new Promise<void>((resolve, reject) => {
        const req = http.request(`${CLEARING_URL}/api/message`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(body), ...AUTH_HEADERS },
        }, (res) => {
          res.on('data', () => {});
          res.on('end', () => resolve());
        });
        req.on('error', reject);
        req.write(body);
        req.end();
      });
    }

    await settle();

    const messages = await getMessages(50);
    for (const role of ['wren', 'silas', 'kade']) {
      const match = messages.find((m: any) => (m.text || '').includes(`${marker}-${role}`));
      expect(match).toBeDefined();
      expect(match.from).toBe(role);
    }
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// AC1: Jeff sends message → correct role receives, response in stream
// ═══════════════════════════════════════════════════════════════════════════

describe('AC1: Jeff sends message → role receives, response appears in stream', () => {
  test('jeff message is recorded in message stream', async () => {
    const marker = `AC1-SEND-${Date.now()}`;

    await sayAsJeff(`${marker} — happy path test`);
    await settle();

    const messages = await getMessages(50);
    const match = messages.find((m: any) => (m.text || '').includes(marker));

    expect(match).toBeDefined();
    expect(match.from).toBe('jeff');
  });

  test('role response appears in message stream', async () => {
    const marker = `AC1-RESPONSE-${Date.now()}`;

    await postMessage('wren', `${marker} — role response`);
    await settle();

    const messages = await getMessages(50);
    const match = messages.find((m: any) => (m.text || '').includes(marker));
    expect(match).toBeDefined();
    expect(match.from).toBe('wren');
  });

  // Previously skipped (#1802/#1813): Socket.IO jeff-message with @mention triggered
  // real osascript injection. Unblocked #2166 — test-mode Clearing spawns with
  // CHORUS_INJECT_DRY_RUN=1 so the shim short-circuits before inject_by_tab_name.
  test('jeff-message via Socket.IO emits to target role (no side effects under dry-run)', async () => {
    const marker = `AC1-SOCKET-${Date.now()}`;
    const client: ClientSocket = createClient();

    await new Promise<void>((resolve, reject) => {
      client.on('connect', () => resolve());
      client.on('connect_error', reject);
      setTimeout(() => reject(new Error('connect timeout')), 5000);
    });

    // Emit jeff-message with @kade mention
    client.emit('jeff-message', { text: `@kade ${marker}` });

    // Give server time to process + persist
    await settle();
    client.disconnect();

    // Verify the message was persisted to the stream
    const messages = await getMessages(50);
    const match = messages.find((m: any) => (m.text || '').includes(marker));
    expect(match).toBeDefined();
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// AC6: Reconnect — no duplicates, no lost messages
// ═══════════════════════════════════════════════════════════════════════════

describe('AC6: Reconnect after disconnect — no duplicates, no lost messages', () => {
  test('messages before and after reconnect each appear exactly once', async () => {
    const marker = `AC6-RECON-${Date.now()}`;

    // Post before and after via REST — no injection side effects
    await sayAsJeff(`${marker}-before`);
    await settle();
    await sayAsJeff(`${marker}-after`);
    await settle();

    // Check: both messages present, each exactly once
    const messages = await getMessages(100);
    const beforeCount = messages.filter((m: any) => (m.text || '').includes(`${marker}-before`)).length;
    const afterCount = messages.filter((m: any) => (m.text || '').includes(`${marker}-after`)).length;

    expect(beforeCount).toBe(1);
    expect(afterCount).toBe(1);
  });

  test('messages sent during disconnect gap are not lost', async () => {
    const marker = `AC6-GAP-${Date.now()}`;

    // Post a message via REST while no Socket.IO client is connected
    const body = JSON.stringify({ from: 'silas', text: `${marker} — sent during gap` });
    await new Promise<void>((resolve, reject) => {
      const req = http.request(`${CLEARING_URL}/api/message`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(body), ...AUTH_HEADERS },
      }, (res) => {
        res.on('data', () => {});
        res.on('end', () => resolve());
      });
      req.on('error', reject);
      req.write(body);
      req.end();
    });

    await settle();

    // New client connects — should receive the gap message in initial state
    const client: ClientSocket = createClient();
    const initialMessages: any[] = [];
    client.on('messages', (msgs: any[]) => {
      initialMessages.push(...msgs);
    });

    await new Promise<void>((resolve, reject) => {
      client.on('connect', resolve);
      client.on('connect_error', reject);
      setTimeout(() => reject(new Error('timeout')), 5000);
    });

    await settle();
    client.disconnect();

    const gapMessage = initialMessages.find((m: any) => (m.text || '').includes(marker));
    expect(gapMessage).toBeDefined();
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// AC8: Session tailer whitelist — only Jeff-facing content passes through
// ═══════════════════════════════════════════════════════════════════════════

describe('AC8: Session tailer whitelist — only Jeff-facing content', () => {
  // These tests verify the filter logic in session-tailer.ts by checking
  // what actually appears in the Clearing after various message types.

  test('jeff-facing message (user typing) passes through', async () => {
    const marker = `AC8-JEFF-${Date.now()}`;

    // Post as jeff input via REST (simulates session tailer forwarding)
    const body = JSON.stringify({ from: 'jeff', text: marker, type: 'jeff-input' });
    await new Promise<void>((resolve, reject) => {
      const req = http.request(`${CLEARING_URL}/api/message`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(body), ...AUTH_HEADERS },
      }, (res) => {
        res.on('data', () => {});
        res.on('end', () => resolve());
      });
      req.on('error', reject);
      req.write(body);
      req.end();
    });

    await settle();
    const messages = await getMessages(50);
    expect(messages.some((m: any) => (m.text || '').includes(marker))).toBe(true);
  });

  test('role response (assistant thinking) passes through', async () => {
    const marker = `AC8-ASSIST-${Date.now()}`;

    const body = JSON.stringify({ from: 'kade', text: marker, type: 'pm-thinking' });
    await new Promise<void>((resolve, reject) => {
      const req = http.request(`${CLEARING_URL}/api/message`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(body), ...AUTH_HEADERS },
      }, (res) => {
        res.on('data', () => {});
        res.on('end', () => resolve());
      });
      req.on('error', reject);
      req.write(body);
      req.end();
    });

    await settle();
    const messages = await getMessages(50);
    expect(messages.some((m: any) => (m.text || '').includes(marker))).toBe(true);
  });

  test('[nudge from] posts appear, typed role-to-role (#3862 ruling)', async () => {
    // Post a role-to-role nudge directly to Clearing API — verify it's filtered
    const marker = `AC8-NUDGE-${Date.now()}`;
    const body = JSON.stringify({ from: 'silas', text: `[nudge from silas] ${marker}`, type: 'role-to-role' });
    await new Promise<void>((resolve, reject) => {
      const req = http.request(`${CLEARING_URL}/api/message`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(body), ...AUTH_HEADERS },
      }, (res) => {
        res.on('data', () => {});
        res.on('end', () => resolve());
      });
      req.on('error', reject);
      req.write(body);
      req.end();
    });

    await settle();
    const messages = await getMessages(100);
    const found = messages.filter((m: any) => (m.text || '').includes(marker));
    expect(found).toHaveLength(1);
    expect(found[0].type).toBe('role-to-role');
  });

  test('DELIVERED confirmations appear, typed role-to-role (#3862 makes role-to-role visible)', async () => {
    const marker = `AC8-DELIV-${Date.now()}`;
    // DELIVERED lines from roles are classified as role-to-role and hidden
    const body = JSON.stringify({ from: 'silas', text: `DELIVERED to wren at ${marker}` });
    await new Promise<void>((resolve, reject) => {
      const req = http.request(`${CLEARING_URL}/api/message`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(body), ...AUTH_HEADERS },
      }, (res) => {
        res.on('data', () => {});
        res.on('end', () => resolve());
      });
      req.on('error', reject);
      req.write(body);
      req.end();
    });

    await settle();
    // #3862 flipped role-to-role to visible; DELIVERED rides the same predicate.
    // If product wants confirmations hidden again, that is a rule change + a
    // ruling, not a test edit.
    const messages = await getMessages(100);
    const deliveredMessages = messages.filter((m: any) =>
      m.from === 'silas' && (m.text || '').includes(`DELIVERED to wren at ${marker}`)
    );
    expect(deliveredMessages).toHaveLength(1);
    expect(deliveredMessages[0].type).toBe('role-to-role');
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// AC7: Background image renders, OG tags correct
// ═══════════════════════════════════════════════════════════════════════════

describe('AC7: Background image and OG tags', () => {
  function getHtml(): Promise<string> {
    return new Promise((resolve, reject) => {
      http.get(`${CLEARING_URL}/`, (res) => {
        let data = '';
        res.on('data', (chunk) => data += chunk);
        res.on('end', () => resolve(data));
      }).on('error', reject);
    });
  }

  test('page title is The Clearing', async () => {
    const html = await getHtml();
    expect(html).toMatch(/<title>.*Clearing.*<\/title>/i);
  });

  test('background image URL present in CSS', async () => {
    const html = await getHtml();
    expect(html).toContain('clearing-bg.jpg');
  });

  test('background image file serves 200', async () => {
    const status = await new Promise<number>((resolve, reject) => {
      http.get(`${CLEARING_URL}/clearing-bg.jpg`, (res) => {
        res.on('data', () => {});
        res.on('end', () => resolve(res.statusCode || 0));
      }).on('error', reject);
    });
    expect(status).toBe(200);
  });

  test('OG title tag is present', async () => {
    const html = await getHtml();
    expect(html).toMatch(/og:title/);
  });

  test('OG image tag is present', async () => {
    const html = await getHtml();
    expect(html).toMatch(/og:image/);
  });

  test('heading shows The Clearing, not Bridge', async () => {
    const html = await getHtml();
    expect(html).toMatch(/The Clearing/);
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// Page structure — every element Jeff sees
// ═══════════════════════════════════════════════════════════════════════════

describe('Page structure — all visible elements', () => {
  function getHtml(): Promise<string> {
    return new Promise((resolve, reject) => {
      http.get(`${CLEARING_URL}/`, (res) => {
        let data = '';
        res.on('data', (chunk) => data += chunk);
        res.on('end', () => resolve(data));
      }).on('error', reject);
    });
  }

  // Header
  test('header contains title "The Clearing"', async () => {
    const html = await getHtml();
    expect(html).toContain('header-title');
    expect(html).toContain('The Clearing');
  });

  test('connection status indicator exists', async () => {
    const html = await getHtml();
    expect(html).toContain('connection-status');
    expect(html).toMatch(/dot-disconnected|dot-connected/);
  });

  // Participant tiles
  test('tiles container exists for role status', async () => {
    const html = await getHtml();
    expect(html).toContain('id="tiles"');
  });

  // Message stream
  test('messages container exists', async () => {
    const html = await getHtml();
    expect(html).toContain('id="messages"');
    expect(html).toContain('id="messages-inner"');
  });

  test('message filter input exists', async () => {
    const html = await getHtml();
    expect(html).toContain('id="msg-filter"');
    expect(html).toContain('filter messages');
  });

  // Send box
  test('input field exists with correct placeholder', async () => {
    const html = await getHtml();
    expect(html).toContain('id="input"');
    expect(html).toContain('@wren @silas @kade');
  });

  test('send button exists', async () => {
    const html = await getHtml();
    expect(html).toContain('id="send-btn"');
    expect(html).toContain('Send');
  });

  test('mic button exists for voice input', async () => {
    const html = await getHtml();
    expect(html).toContain('id="mic-btn"');
  });

  test('input hint shows routing instructions', async () => {
    const html = await getHtml();
    expect(html).toContain('input-hint');
    expect(html).toContain('@role to direct');
    expect(html).toContain('Wren gets it');
  });

  // Domains panel
  test('domains panel exists with role filter buttons', async () => {
    const html = await getHtml();
    expect(html).toContain('id="flow-pane"');
    expect(html).toContain('Domains');
    expect(html).toContain('data-filter="wren"');
    expect(html).toContain('data-filter="silas"');
    expect(html).toContain('data-filter="kade"');
  });

  test('domain filter input exists', async () => {
    const html = await getHtml();
    expect(html).toContain('id="domain-filter"');
    expect(html).toContain('filter domains');
  });

  // Streams panel (fold)
  test('streams fold panel exists', async () => {
    const html = await getHtml();
    expect(html).toContain('id="fold-panel"');
    expect(html).toContain('id="fold-content"');
  });

  // Mobile tabs
  test('mobile tabs exist for Messages/Domains/Streams', async () => {
    const html = await getHtml();
    expect(html).toContain('id="mobile-tabs"');
    expect(html).toContain('data-panel="messages"');
    expect(html).toContain('data-panel="domains"');
    expect(html).toContain('data-panel="streams"');
  });

  // Socket.IO
  test('Socket.IO client script is loaded', async () => {
    const html = await getHtml();
    expect(html).toContain('socket.io/socket.io.js');
  });

  test('Socket.IO reconnection configured', async () => {
    const html = await getHtml();
    expect(html).toContain('reconnection: true');
    expect(html).toContain('reconnectionAttempts: Infinity');
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// Message rendering — role names, timestamps, content
// ═══════════════════════════════════════════════════════════════════════════

describe('Message rendering via API', () => {
  test('messages have from, text, ts, and type fields', async () => {
    const marker = `RENDER-${Date.now()}`;
    const body = JSON.stringify({ from: 'silas', text: marker });
    await new Promise<void>((resolve, reject) => {
      const req = http.request(`${CLEARING_URL}/api/message`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(body), ...AUTH_HEADERS },
      }, (res) => {
        res.on('data', () => {});
        res.on('end', () => resolve());
      });
      req.on('error', reject);
      req.write(body);
      req.end();
    });

    await settle();
    const messages = await getMessages(10);
    const match = messages.find((m: any) => (m.text || '').includes(marker));

    expect(match).toBeDefined();
    expect(match).toHaveProperty('from', 'silas');
    expect(match).toHaveProperty('text');
    expect(match).toHaveProperty('ts');
    expect(match).toHaveProperty('type');
  });

  test('messages from each role have correct attribution', async () => {
    const marker = `ATTR-${Date.now()}`;
    const roles = ['wren', 'silas', 'kade', 'jeff'];

    for (const role of roles) {
      const body = JSON.stringify({ from: role, text: `${marker}-${role}` });
      await new Promise<void>((resolve, reject) => {
        const req = http.request(`${CLEARING_URL}/api/message`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(body), ...AUTH_HEADERS },
        }, (res) => {
          res.on('data', () => {});
          res.on('end', () => resolve());
        });
        req.on('error', reject);
        req.write(body);
        req.end();
      });
    }

    await settle();
    const messages = await getMessages(50);

    for (const role of roles) {
      const match = messages.find((m: any) => (m.text || '').includes(`${marker}-${role}`));
      expect(match).toBeDefined();
      expect(match.from).toBe(role);
    }
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// API endpoints — health, tiles, messages, streams
// ═══════════════════════════════════════════════════════════════════════════

describe('API endpoints', () => {
  test('GET /health returns ok', async () => {
    const result = await new Promise<any>((resolve, reject) => {
      http.get(`${CLEARING_URL}/health`, (res) => {
        let data = '';
        res.on('data', (chunk) => data += chunk);
        res.on('end', () => resolve(JSON.parse(data)));
      }).on('error', reject);
    });
    expect(result.status).toBe('ok');
    expect(result.port).toBe(TEST_PORT);
  });

  test('GET /api/messages returns array', async () => {
    const messages = await getMessages(5);
    expect(Array.isArray(messages)).toBe(true);
  });

  test('GET /api/messages returns messages', async () => {
    const messages = await getMessages(50);
    expect(messages.length).toBeGreaterThan(0);
  });

  test('POST /api/message requires from and text', async () => {
    const status = await new Promise<number>((resolve, reject) => {
      const body = JSON.stringify({ from: 'test' }); // missing text
      const req = http.request(`${CLEARING_URL}/api/message`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(body), ...AUTH_HEADERS },
      }, (res) => {
        res.on('data', () => {});
        res.on('end', () => resolve(res.statusCode || 0));
      });
      req.on('error', reject);
      req.write(body);
      req.end();
    });
    expect(status).toBe(400);
  });

  test('Socket.IO emits tiles on connect', async () => {
    const client = createClient();
    const tiles = await new Promise<any>((resolve, reject) => {
      client.on('connect', () => {});
      client.on('tiles', (data: any) => resolve(data));
      client.on('connect_error', reject);
      setTimeout(() => reject(new Error('no tiles received')), 5000);
    });
    client.disconnect();

    expect(tiles).toBeDefined();
  });

  test('Socket.IO emits messages on connect', async () => {
    const client = createClient();
    const msgs = await new Promise<any>((resolve, reject) => {
      client.on('messages', (data: any) => resolve(data));
      client.on('connect_error', reject);
      setTimeout(() => reject(new Error('no messages received')), 5000);
    });
    client.disconnect();

    expect(Array.isArray(msgs)).toBe(true);
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// Error states
// ═══════════════════════════════════════════════════════════════════════════

describe('Error states', () => {
  test('404 for unknown routes', async () => {
    const status = await new Promise<number>((resolve, reject) => {
      http.get(`${CLEARING_URL}/nonexistent-page`, (res) => {
        res.on('data', () => {});
        res.on('end', () => resolve(res.statusCode || 0));
      }).on('error', reject);
    });
    expect(status).toBe(404);
  });

  test('POST /api/message with empty body returns 400', async () => {
    const status = await new Promise<number>((resolve, reject) => {
      const body = '{}';
      const req = http.request(`${CLEARING_URL}/api/message`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(body), ...AUTH_HEADERS },
      }, (res) => {
        res.on('data', () => {});
        res.on('end', () => resolve(res.statusCode || 0));
      });
      req.on('error', reject);
      req.write(body);
      req.end();
    });
    expect(status).toBe(400);
  });
});
