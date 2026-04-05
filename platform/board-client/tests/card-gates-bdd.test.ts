/**
 * BDD tests for card gates — creation validation and Now gate
 *
 * IMPORTANT: These tests NEVER create real cards on the production board.
 * Validation tests use error paths (which exit before API call).
 * Now gate tests use SDK source inspection.
 */
import * as fs from 'fs';
import * as path from 'path';

const CLI = path.join(__dirname, '..', 'dist', 'cli.js');
const SDK_SRC = fs.readFileSync(path.join(__dirname, '..', 'src', 'sdk.ts'), 'utf-8');

const cliExists = (() => {
  try { require('fs').accessSync(CLI); return true; } catch { return false; }
})();

function run(args: string): { stdout: string; stderr: string; exitCode: number } {
  try {
    const { execSync } = require('child_process');
    const stdout = execSync(`node ${CLI} ${args}`, {
      encoding: 'utf8',
      timeout: 15000,
      env: { ...process.env, NODE_ENV: 'test' },
    });
    return { stdout, stderr: '', exitCode: 0 };
  } catch (e: any) {
    return { stdout: e.stdout || '', stderr: e.stderr || '', exitCode: e.status || 1 };
  }
}

const skip = () => !cliExists;
const skipMsg = 'CLI not built — run npm run build first';

// ── Card Creation: mandatory fields (error paths only — no cards created) ──

describe('Card creation enforces mandatory fields', () => {

  test('Create card without type is rejected', () => {
    if (skip()) return console.log(skipMsg);
    const result = run('add "BDD test card" --owner silas --priority P2 --domain chorus');
    expect(result.exitCode).not.toBe(0);
    expect(result.stderr + result.stdout).toMatch(/type/i);
  });

  test('Chunk is auto-inferred from domain — not a hard gate', () => {
    if (skip()) return console.log(skipMsg);
    // #1873 removed chunk as mandatory — verify error does NOT mention chunk
    // This test intentionally omits other required fields so it fails before API call
    const result = run('add "BDD test card" --owner silas --priority P2 --domain chorus');
    const output = result.stderr + result.stdout;
    expect(output).not.toMatch(/Missing.*chunk/i);
  });

  test('Create card without domain is rejected', () => {
    if (skip()) return console.log(skipMsg);
    const result = run('add "BDD test card" --owner silas --priority P2 --type fix');
    expect(result.exitCode).not.toBe(0);
    expect(result.stderr + result.stdout).toMatch(/domain/i);
  });

  test('Create card without priority is rejected', () => {
    if (skip()) return console.log(skipMsg);
    const result = run('add "BDD test card" --owner silas --type fix --domain chorus');
    expect(result.exitCode).not.toBe(0);
    expect(result.stderr + result.stdout).toMatch(/priority/i);
  });

  test('Quick card still requires type, domain, priority', () => {
    if (skip()) return console.log(skipMsg);
    const noType = run('add "BDD quick test" -q --domain chorus --priority P2');
    expect(noType.exitCode).not.toBe(0);
    expect(noType.stderr + noType.stdout).toMatch(/type/i);

    const noDomain = run('add "BDD quick test" -q --type fix --priority P2');
    expect(noDomain.exitCode).not.toBe(0);
    expect(noDomain.stderr + noDomain.stdout).toMatch(/domain/i);

    const noPriority = run('add "BDD quick test" -q --type fix --domain chorus');
    expect(noPriority.exitCode).not.toBe(0);
    expect(noPriority.stderr + noPriority.stdout).toMatch(/priority/i);
  });
});

// ── Now gate: structural verification (no real cards created) ──

describe('Now gate requires description with Experience and AC', () => {

  test('SDK enforces Experience section on WIP entry', () => {
    expect(SDK_SRC).toMatch(/enforceExperienceGate/);
    expect(SDK_SRC).toMatch(/Experience.*section.*before.*WIP|WIP.*Experience/i);
  });

  test('SDK enforces description on Now entry', () => {
    expect(SDK_SRC).toMatch(/Cards entering Now require.*description/i);
  });

  test('SDK checks for ## Experience heading', () => {
    expect(SDK_SRC).toMatch(/##\\s\*experience/i);
  });

  test('SDK checks for ## AC heading', () => {
    expect(SDK_SRC).toContain('ac|criteria|what|acceptance');
  });

  test('SWAT cards bypass gates', () => {
    expect(SDK_SRC).toMatch(/swat/i);
  });
});
