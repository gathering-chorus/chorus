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

// Read SDK source to verify inference logic structurally
const SDK_SRC = fs.readFileSync(path.join(__dirname, '..', 'src', 'sdk.ts'), 'utf-8');

describe('Origin required at card creation', () => {
  test('cards add without --origin and non-inferable type (chore) fails with clear error', () => {
    try {
      execSync(
        `node ${CLI} add "Clean up test artifacts" --owner silas --priority P2 --domain chorus --type chore --quick 2>&1`,
        { encoding: 'utf-8', timeout: 10000 }
      );
      throw new Error('Should have exited non-zero');
    } catch (err: any) {
      const output = (err.stdout || '') + (err.stderr || '');
      expect(output).toMatch(/origin/i);
      expect(output).toMatch(/reactive.*reflective|reflective.*reactive/i);
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
      execSync(`node ${CLI} add 2>&1`, { encoding: 'utf-8', timeout: 10000 });
    } catch (err: any) {
      const output = (err.stdout || '') + (err.stderr || '');
      expect(output).toMatch(/--origin/);
    }
  });
});
