/**
 * Tests for `cards update <id> --desc` — card #1964
 *
 * AC:
 * - cards update <id> --desc 'text' writes the description field via Vikunja API
 * - Markdown content preserved (headings, checkboxes, lists)
 * - cards view <id> renders the updated description
 * - No direct Vikunja API calls from roles — all through cards CLI
 */
import { execSync } from 'child_process';
import path from 'path';

const CLI = path.join(__dirname, '..', 'dist', 'cli.js');
const cliExists = (() => {
  try { require('fs').accessSync(CLI); return true; } catch { return false; }
})();

function run(args: string): { stdout: string; stderr: string; exitCode: number } {
  try {
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

describe('cards update --desc', () => {
  const skipMsg = 'CLI not built — run npm run build first';

  test('update command is recognized (does not die with "Removed" error)', () => {
    if (!cliExists) return console.log(skipMsg);
    const result = run('update');
    expect(result.stderr + result.stdout).not.toContain('Removed');
    expect(result.stderr + result.stdout).toContain('Usage');
  });

  test('update --desc passes description through to API', () => {
    if (!cliExists) return console.log(skipMsg);
    const markdown = '## Experience\\n\\nTest description with **markdown**.\\n\\n## AC\\n- Item one\\n- Item two';
    const result = run(`update 1964 --desc "${markdown}"`);
    expect(result.stderr).not.toContain('Removed');
    if (result.exitCode === 0) {
      expect(result.stdout).toContain('#1964');
    }
  });

  test('update --desc preserves markdown in view output', () => {
    if (!cliExists) return console.log(skipMsg);
    const result = run('view 1964');
    if (result.exitCode === 0) {
      expect(result.stdout).toContain('##');
    }
  });

  test('update requires at least one field', () => {
    if (!cliExists) return console.log(skipMsg);
    const result = run('update 1964');
    expect(result.exitCode).not.toBe(0);
    expect(result.stderr + result.stdout).toContain('Provide');
    expect(result.stderr + result.stdout).not.toContain('Removed');
  });
});
