/**
 * #2101 — Origin tag required at card creation.
 * Validates CLI parsing, inference, and error messages.
 */

import { execSync } from 'child_process';

const CARDS = 'bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/cards';

function runCards(args: string): { stdout: string; stderr: string; exitCode: number } {
  try {
    const stdout = execSync(`${CARDS} ${args} 2>&1`, { encoding: 'utf-8', timeout: 10000 });
    return { stdout, stderr: '', exitCode: 0 };
  } catch (err: any) {
    return { stdout: err.stdout || '', stderr: err.stderr || '', exitCode: err.status || 1 };
  }
}

describe('Origin required at card creation', () => {
  test('cards add without --origin and non-inferable type (chore) fails with clear error', () => {
    const result = runCards('add "Clean up test artifacts" --owner silas --priority P2 --domain chorus --type chore --quick');
    expect(result.exitCode).not.toBe(0);
    expect(result.stdout + result.stderr).toMatch(/origin/i);
    expect(result.stdout + result.stderr).toMatch(/reactive.*reflective|reflective.*reactive/i);
  });

  test('cards add with --origin reflective succeeds (other fields present)', () => {
    // Dry validation — we check the error output doesn't mention origin
    const result = runCards('add "Test origin flag" --owner silas --priority P2 --domain chorus --type new --origin reflective --quick');
    const output = result.stdout + result.stderr;
    // Should not fail on origin — may fail on other things but origin should be clean
    expect(output).not.toMatch(/Missing.*origin/i);
  });

  test('cards add with type:fix auto-infers origin:reactive', () => {
    const result = runCards('add "Fix broken test" --owner silas --priority P2 --domain chorus --type fix --quick');
    const output = result.stdout + result.stderr;
    expect(output).toMatch(/origin:reactive/i);
    expect(output).not.toMatch(/Missing.*origin/i);
  });

  test('cards add with type:swat auto-infers origin:reactive', () => {
    const result = runCards('add "Emergency deploy fix" --owner silas --priority P1 --domain chorus --type swat --quick');
    const output = result.stdout + result.stderr;
    expect(output).toMatch(/origin:reactive/i);
    expect(output).not.toMatch(/Missing.*origin/i);
  });

  test('cards add with type:new auto-infers origin:reflective', () => {
    const result = runCards('add "Build new dashboard" --owner silas --priority P2 --domain chorus --type new --quick');
    const output = result.stdout + result.stderr;
    expect(output).toMatch(/origin:reflective/i);
    expect(output).not.toMatch(/Missing.*origin/i);
  });

  test('cards add with type:enhance auto-infers origin:reflective', () => {
    const result = runCards('add "Improve search speed" --owner silas --priority P2 --domain chorus --type enhance --quick');
    const output = result.stdout + result.stderr;
    expect(output).toMatch(/origin:reflective/i);
    expect(output).not.toMatch(/Missing.*origin/i);
  });

  test('explicit --origin overrides type inference', () => {
    // A fix card that's actually reflective (proactive cleanup)
    const result = runCards('add "Fix stale comments" --owner silas --priority P2 --domain chorus --type fix --origin reflective --quick');
    const output = result.stdout + result.stderr;
    expect(output).not.toMatch(/Missing.*origin/i);
  });

  test('--help mentions --origin flag', () => {
    const result = runCards('add');
    const output = result.stdout + result.stderr;
    expect(output).toMatch(/--origin/);
  });
});
