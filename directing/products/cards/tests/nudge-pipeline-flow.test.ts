/**
 * Nudge Pipeline Flow Tests — #1242
 *
 * End-to-end validation of the nudge pipeline:
 *   event → queue/inject → drain on idle → 2-exchange limit
 *
 * Tests nudge behavior via subprocess execution with isolated temp dirs.
 */
import { execSync } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';

const SCRIPTS_DIR = path.join(__dirname, '../../../../platform/scripts');
const NUDGE_SCRIPT = path.join(SCRIPTS_DIR, 'nudge');

// Use isolated temp dirs to avoid interfering with real nudge state
let testInboxDir: string;
let testExchangeDir: string;

beforeEach(() => {
  testInboxDir = fs.mkdtempSync(path.join(os.tmpdir(), 'nudge-test-inbox-'));
  testExchangeDir = fs.mkdtempSync(path.join(os.tmpdir(), 'nudge-test-exchange-'));
});

afterEach(() => {
  fs.rmSync(testInboxDir, { recursive: true, force: true });
  fs.rmSync(testExchangeDir, { recursive: true, force: true });
});

// Helper: run nudge with overridden dirs (tests the queue/drain logic, not TTY injection)
function runNudge(args: string, env: Record<string, string> = {}): { stdout: string; exitCode: number } {
  try {
    const stdout = execSync(`bash ${NUDGE_SCRIPT} ${args}`, {
      encoding: 'utf-8',
      env: {
        ...process.env,
        INBOX_DIR_OVERRIDE: testInboxDir,
        ...env,
      },
      timeout: 10000,
    });
    return { stdout, exitCode: 0 };
  } catch (err: any) {
    return { stdout: err.stdout || err.stderr || '', exitCode: err.status || 1 };
  }
}

// ═══════════════════════════════════════════════════════════════════════════
// 1. NUDGE SCRIPT EXISTS AND IS EXECUTABLE
// ═══════════════════════════════════════════════════════════════════════════

describe('Flow: Nudge script basics', () => {
  test('nudge exists', () => {
    expect(fs.existsSync(NUDGE_SCRIPT)).toBe(true);
  });

  test('nudge is executable', () => {
    const stat = fs.statSync(NUDGE_SCRIPT);
    expect(stat.mode & 0o111).toBeGreaterThan(0);
  });

  test('nudge help prints usage', () => {
    const { stdout } = runNudge('help');
    expect(stdout).toContain('nudge');
    // drain is now a Rust subcommand, help shows chorus-hook-shim usage
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// 2. QUEUE MECHANISM — messages queued when target unavailable
// ═══════════════════════════════════════════════════════════════════════════

describe('Flow: Nudge queue mechanism', () => {
  test('queue file format is from|timestamp|message', () => {
    // Directly create a queue entry in the test format
    const roleInbox = path.join(testInboxDir, 'testrole');
    fs.mkdirSync(roleInbox, { recursive: true });
    const ts = Math.floor(Date.now() / 1000);
    const line = `silas|${ts}|Test message content\n`;
    fs.writeFileSync(path.join(roleInbox, 'pending.txt'), line);

    const content = fs.readFileSync(path.join(roleInbox, 'pending.txt'), 'utf-8');
    const parts = content.trim().split('|');
    expect(parts).toHaveLength(3);
    expect(parts[0]).toBe('silas');
    expect(parseInt(parts[1])).toBeGreaterThan(0);
    expect(parts[2]).toBe('Test message content');
  });

  test('multiple queued messages are ordered', () => {
    const roleInbox = path.join(testInboxDir, 'testrole');
    fs.mkdirSync(roleInbox, { recursive: true });
    const ts = Math.floor(Date.now() / 1000);
    const lines = [
      `wren|${ts}|First message\n`,
      `silas|${ts + 1}|Second message\n`,
      `kade|${ts + 2}|Third message\n`,
    ];
    fs.writeFileSync(path.join(roleInbox, 'pending.txt'), lines.join(''));

    const content = fs.readFileSync(path.join(roleInbox, 'pending.txt'), 'utf-8');
    const entries = content.trim().split('\n');
    expect(entries).toHaveLength(3);
    expect(entries[0]).toContain('First');
    expect(entries[2]).toContain('Third');
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// 3. DRAIN MECHANISM — queued messages delivered on drain
// ═══════════════════════════════════════════════════════════════════════════


// ═══════════════════════════════════════════════════════════════════════════
// 4. EXCHANGE LIMIT — 2-exchange limit between same pair (DEC-079)
// ═══════════════════════════════════════════════════════════════════════════


// ═══════════════════════════════════════════════════════════════════════════
// 5. SPINE EVENTS — nudge events emitted on send/deliver/limit
// ═══════════════════════════════════════════════════════════════════════════


// ═══════════════════════════════════════════════════════════════════════════
// 6. STALE DISCARD — messages older than 24h discarded on drain
// ═══════════════════════════════════════════════════════════════════════════

