// @test-type: integration — shells out to the real cards CLI (execSync) and
// @domain: cards — placed by chorus-crawl's domain rules (#4538)
// asserts on its refusal messages; gated behind RUN_INTEGRATION. Creates no
// cards: every case is an error path.
/**
 * #2101 — Origin tag required at card creation.
 * Validates CLI parsing, inference, and error messages.
 *
 * IMPORTANT: These tests verify logic structurally (source inspection)
 * or via error paths (which don't create cards). Tests NEVER create
 * real cards on the production board.
 */

import { execSync } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';

const CLI = path.join(__dirname, '..', 'dist', 'cli.js');
// #4628 — these tests spawn the BUILT CLI. A werk the pipeline tests has no
// dist, so they failed with "Cannot find module …/dist/cli.js" (#4538 on 10-15,
// #4628 on 10-19) — a red about the box, not the product. Build it once if it is
// missing; a build that fails says so, loudly, instead of a confusing toMatch.
beforeAll(() => {
  if (fs.existsSync(CLI)) return;
  try {
    execSync('npx tsc -p .', { cwd: path.join(__dirname, '..'), encoding: 'utf-8', timeout: 120000, stdio: 'pipe' });
  } catch (e) {
    throw new Error(`dist/cli.js is missing and building it failed — build first: ${(e as { stdout?: string }).stdout ?? e}`);
  }
  if (!fs.existsSync(CLI)) throw new Error('dist/cli.js is missing after `tsc -p .` — build first');
  // slow-by-design: npx tsc -p . (cold dist build) measured 4.5 s typecheck on Library 10-26; a loaded nightly box runs it many times slower
}, 150000);
// #4538 — the CLI reads Vikunja settings before it validates arguments, so in a
// werk (no .env) it died with "No .env file found" and these validation tests
// went red for a machine reason. They get an obviously fake token and a dead
// port, so the checks run without the box and can never reach the live board.
const HERMETIC_ENV = {
  ...process.env,
  NODE_ENV: 'test',
  VIKUNJA_TOKEN: ['fixture', 'cards', 'test', '4538'].join('-'),
  VIKUNJA_URL: 'http://127.0.0.1:9',
};


// Read SDK source to verify inference logic structurally
const SDK_SRC = fs.readFileSync(path.join(__dirname, '..', 'src', 'sdk.ts'), 'utf-8');

describe('Origin required at card creation', () => {
  test('cards add without --origin and non-inferable type (chore) fails with clear error', () => {
    try {
      execSync(
        `node ${CLI} add "Clean up test artifacts" --owner silas --priority P2 --domain chorus --type chore 2>&1`,
        { encoding: 'utf-8', timeout: 10000, env: HERMETIC_ENV }
      );
      throw new Error('Should have exited non-zero');
    } catch (err: any) {
      const output = (err.stdout || '') + (err.stderr || '');
      /* eslint-disable jest/no-conditional-expect -- exec is expected to throw; assertions inspect error output */
      expect(output).toMatch(/origin/i);
      expect(output).toMatch(/reactive.*reflective|reflective.*reactive/i);
      /* eslint-enable jest/no-conditional-expect */
    }
  });

  test('SDK has TYPE_TO_ORIGIN inference map with fix→reactive', () => {
    expect(SDK_SRC).toContain("fix: 'reactive'");
  });

  test('SDK has TYPE_TO_ORIGIN inference map with swat→reactive', () => {
    expect(SDK_SRC).toContain("swat: 'reactive'");
  });

  test('SDK has TYPE_TO_ORIGIN inference map with new→reflective', () => {
    expect(SDK_SRC).toContain("new: 'reflective'");
  });

  test('SDK has TYPE_TO_ORIGIN inference map with enhance→reflective', () => {
    expect(SDK_SRC).toContain("enhance: 'reflective'");
  });

  test('SDK validates origin values (reflective or reactive only)', () => {
    expect(SDK_SRC).toMatch(/Unknown origin/);
    expect(SDK_SRC).toMatch(/\['reflective', 'reactive'\]/);
  });

  test('SDK auto-tags origin after card creation', () => {
    expect(SDK_SRC).toMatch(/client\.tag\(task\.index, 'origin'/);
  });

  test('--help mentions --origin flag', () => {
    try {
      execSync(`node ${CLI} add 2>&1`, { encoding: 'utf-8', timeout: 10000, env: HERMETIC_ENV });
    } catch (err: any) {
      const output = (err.stdout || '') + (err.stderr || '');
      // eslint-disable-next-line jest/no-conditional-expect -- exec expected to throw; assert error output
      expect(output).toMatch(/--origin/);
    }
  });
});
