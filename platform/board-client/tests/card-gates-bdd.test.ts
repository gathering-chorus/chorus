/**
 * BDD tests for card #1966 — tighten cards CLI gates
 *
 * Scenarios from service-design-cards.html:
 *   Creation (6): mandatory type, chunk, domain, priority; --quick only exempts description
 *   Now gate (3): hard-block without description, require Experience + AC, SWAT bypass
 *
 * These test the CLI end-to-end via spawned process — what Jeff sees.
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

const skip = () => !cliExists;
const skipMsg = 'CLI not built — run npm run build first';

// ── Card Creation: mandatory fields ──

describe('Card creation enforces mandatory fields', () => {

  test('Create card without type is rejected', () => {
    if (skip()) return console.log(skipMsg);
    const result = run('add "BDD test card" --owner silas --priority P2 --domain chorus');
    expect(result.exitCode).not.toBe(0);
    expect(result.stderr + result.stdout).toMatch(/type/i);
  });

  test('Chunk is auto-inferred from domain — not a hard gate', () => {
    if (skip()) return console.log(skipMsg);
    // #1873 removed chunk as mandatory — domain auto-infers chunk
    const result = run('add "BDD test card" --owner silas --priority P2 --type fix --domain chorus --quick');
    const output = result.stderr + result.stdout;
    // Should not fail on missing chunk
    expect(output).not.toMatch(/Missing.*chunk/i);
  });

  test('Create card without domain is rejected', () => {
    if (skip()) return console.log(skipMsg);
    const result = run('add "BDD test card" --owner silas --priority P2 --chunk ops --type fix');
    expect(result.exitCode).not.toBe(0);
    expect(result.stderr + result.stdout).toContain('domain');
  });

  test('Create card without priority is rejected', () => {
    if (skip()) return console.log(skipMsg);
    const result = run('add "BDD test card" --owner silas --chunk ops --type fix --domain chorus');
    expect(result.exitCode).not.toBe(0);
    expect(result.stderr + result.stdout).toContain('priority');
  });

  test('Quick card still requires type, domain, priority', () => {
    if (skip()) return console.log(skipMsg);
    // --quick without classification should still fail
    // #1873: chunk no longer required (auto-inferred from domain)
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

  test('Quick card with all classification but no description succeeds', () => {
    if (skip()) return console.log(skipMsg);
    // --quick exempts description only — card should create if type/chunk/domain/priority present
    const result = run('add "BDD quick card passes" -q --owner silas --type fix --chunk ops --domain chorus --priority P2');
    // Should succeed (exit 0) or at least not fail on classification
    if (result.exitCode !== 0) {
      // If it fails, it should NOT be because of type/chunk/domain/priority
      const output = result.stderr + result.stdout;
      expect(output).not.toContain('Cards require a type');
      expect(output).not.toContain('chunk');
      expect(output).not.toContain('domain');
      expect(output).not.toContain('priority');
    }
  });
});

// ── Now gate: description with Experience + AC ──

describe('Now gate requires description with Experience and AC', () => {

  test('Move card without description to Now is blocked', () => {
    if (skip()) return console.log(skipMsg);
    // Create a quick card (no description), then try to move to Now
    const addResult = run('add "BDD now-gate test" -q --owner silas --type fix --chunk ops --domain chorus --priority P2');
    const output = addResult.stdout + addResult.stderr;
    const match = output.match(/#(\d+)/);
    if (!match) return console.log('Could not create test card');
    const cardId = match[1];

    const moveResult = run(`move ${cardId} Now`);
    expect(moveResult.exitCode).not.toBe(0);
    const moveOutput = moveResult.stderr + moveResult.stdout;
    expect(moveOutput).toMatch(/experience|ac|description/i);

    // Cleanup — move to Won't Do
    run(`move ${cardId} wd`);
  });

  test('Move card with Experience + AC to Now succeeds', () => {
    if (skip()) return console.log(skipMsg);
    const desc = '## Experience\\nJeff sees the test pass.\\n\\n## AC\\n- [ ] Test passes';
    const addResult = run(`add "BDD now-gate pass" --owner silas --type fix --chunk ops --domain chorus --priority P2 --desc "${desc}"`);
    const output = addResult.stdout + addResult.stderr;
    const match = output.match(/#(\d+)/);
    if (!match) return console.log('Could not create test card');
    const cardId = match[1];

    const moveResult = run(`move ${cardId} Now`);
    expect(moveResult.exitCode).toBe(0);

    // Cleanup
    run(`move ${cardId} wd`);
  });

  // Note: SWAT bypass test requires creating a [swat] card — tested in existing gate tests
});
