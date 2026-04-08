/**
 * Brief Directory Restructure Test — #1802
 *
 * Validates that brief routing uses the canonical platform/roles/<name>/briefs paths,
 * not the legacy root-level architect/engineer/product-manager directories.
 */
import * as fs from 'fs';
import * as path from 'path';

const PROJECT_ROOT = path.join(__dirname, '../../../..');
const SDK_PATH = path.join(__dirname, '../src/sdk.ts');

describe('#1802: Brief directories use canonical paths', () => {
  test('platform/roles/silas/briefs exists', () => {
    expect(fs.existsSync(path.join(PROJECT_ROOT, 'roles/silas/briefs'))).toBe(true);
  });

  test('platform/roles/kade/briefs exists', () => {
    expect(fs.existsSync(path.join(PROJECT_ROOT, 'roles/kade/briefs'))).toBe(true);
  });

  test('platform/roles/wren/briefs exists', () => {
    expect(fs.existsSync(path.join(PROJECT_ROOT, 'roles/wren/briefs'))).toBe(true);
  });

  test('sdk.ts BRIEF_DIRS uses roles/<name>/briefs, not root-level dirs', () => {
    const sdk = fs.readFileSync(SDK_PATH, 'utf-8');
    // Should reference roles/silas, roles/kade, roles/wren
    expect(sdk).toContain('roles/silas/briefs');
    expect(sdk).toContain('roles/kade/briefs');
    expect(sdk).toContain('roles/wren/briefs');
    // Should NOT reference old root-level directories
    expect(sdk).not.toMatch(/['"].*architect\/briefs['"]/);
    expect(sdk).not.toMatch(/['"].*engineer\/briefs['"]/);
    expect(sdk).not.toMatch(/['"].*product-manager\/briefs['"]/);
  });
});
