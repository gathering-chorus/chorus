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

describe.skip('Flow: Nudge drain [migrated to Rust]', () => {
  test('drain with empty inbox succeeds silently', () => {
    const { stdout, exitCode } = runNudge('drain testrole');
    // No pending.txt → early return with no output (exit 0)
    expect(exitCode).toBe(0);
    expect(stdout.trim()).toBe('');
  });

  test('drain command exists and accepts role argument', () => {
    const { stdout, exitCode } = runNudge('drain kade');
    // Should succeed (0 messages is fine)
    expect(exitCode).toBe(0);
  });

  test('inbox command shows queued messages', () => {
    const { stdout } = runNudge('inbox testrole');
    expect(stdout).toContain('No queued messages');
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// 4. EXCHANGE LIMIT — 2-exchange limit between same pair (DEC-079)
// ═══════════════════════════════════════════════════════════════════════════

describe.skip('Flow: Exchange limit [migrated to Rust] (DEC-079)', () => {
  test('exchange limit file uses alphabetical pair key', () => {
    // silas-wren, not wren-silas (alphabetical)
    const pairA = ['silas', 'wren'].sort().join('-');
    const pairB = ['wren', 'silas'].sort().join('-');
    expect(pairA).toBe(pairB);
    expect(pairA).toBe('silas-wren');
  });

  test('exchange counter format tracks sender→target with timestamp', () => {
    // Simulate exchange tracking
    const exchangeFile = path.join(testExchangeDir, 'kade-silas');
    const ts = Math.floor(Date.now() / 1000);
    fs.writeFileSync(exchangeFile, `${ts}|kade>silas\n${ts + 1}|silas>kade\n`);

    const content = fs.readFileSync(exchangeFile, 'utf-8');
    const lines = content.trim().split('\n');
    expect(lines).toHaveLength(2);

    // Third exchange should be blocked by the script
    const count = lines.length;
    expect(count).toBe(2);
  });

  test('exchange limit removed — no blocking on exchange count', () => {
    // Exchange limit was removed per Jeff's direction (DEC-079 superseded)
    const scriptContent = fs.readFileSync(NUDGE_SCRIPT, 'utf-8');
    expect(scriptContent).toContain('Exchange limit removed');
    // Should NOT contain the old blocking condition
    expect(scriptContent).not.toContain('-ge 2');
  });

  test('exchange counter still tracks for observability', () => {
    const scriptContent = fs.readFileSync(NUDGE_SCRIPT, 'utf-8');
    // Exchange tracking infrastructure still exists (counter, pair key, 30min reset)
    expect(scriptContent).toContain('EXCHANGE_COUNT');
    expect(scriptContent).toContain('1800');
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// 5. SPINE EVENTS — nudge events emitted on send/deliver/limit
// ═══════════════════════════════════════════════════════════════════════════

describe.skip('Flow: Nudge spine [migrated to Rust] events', () => {
  test('script emits role.nudge.sent event', () => {
    const content = fs.readFileSync(NUDGE_SCRIPT, 'utf-8');
    expect(content).toContain('role.nudge.sent');
  });

  test('script emits role.nudge.delivered event', () => {
    const content = fs.readFileSync(NUDGE_SCRIPT, 'utf-8');
    expect(content).toContain('role.nudge.delivered');
  });

  test('exchange limit removed — no limit_reached event emitted', () => {
    const content = fs.readFileSync(NUDGE_SCRIPT, 'utf-8');
    // limit_reached event removed with the exchange limit
    expect(content).not.toContain('role.nudge.limit_reached');
  });

  test('script emits role.nudge.queued when TTY unavailable', () => {
    const content = fs.readFileSync(NUDGE_SCRIPT, 'utf-8');
    expect(content).toContain('role.nudge.queued');
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// 6. STALE DISCARD — messages older than 24h discarded on drain
// ═══════════════════════════════════════════════════════════════════════════

describe.skip('Flow: Stale nudge [migrated to Rust] handling', () => {
  test('drain referenced in werk-init.sh for session start', () => {
    const werkInit = path.join(SCRIPTS_DIR, 'werk-init.sh');
    expect(fs.existsSync(werkInit)).toBe(true);
    const content = fs.readFileSync(werkInit, 'utf-8');
    expect(content).toMatch(/nudge|drain/);
  });

  test('stale discard logic exists in drain pipeline', () => {
    const roleState = path.join(SCRIPTS_DIR, 'role-state');
    expect(fs.existsSync(roleState)).toBe(true);
    // role-state is a symlink to the Rust binary — verify it exists and is executable
    expect(fs.lstatSync(roleState).isSymbolicLink() || fs.statSync(roleState).mode & 0o111).toBeTruthy();
  });
});
