// @test-type: unit — pure tests of the tagger/gate/fitness logic; the security/api/fs tokens below are FIXTURE INPUTS, not real calls.
// #3442 — content-signal test-type tagger.
// Supersedes discover-tests.ts:classifyTestType (path-only heuristic) which
// mislabeled 7 *-unit.test.ts files that touch real fs as "unit". The tagger
// reads MECHANICAL CONTENT SIGNALS, not the path: a test that grabs a real
// tmpdir IS integration even if its filename says -unit.
import { tagTestType } from '../src/tag-test-type';

describe('tagTestType — content signals', () => {
  it('THE DRIFT CASE: a *-unit.test.ts touching real fs is integration, not unit', () => {
    const content = `
      import { mkdtempSync } from 'fs';
      const dir = mkdtempSync('/tmp/foo-');
    `;
    // filename lies ("unit"); the signal (mkdtemp) is the truth.
    expect(tagTestType(content, 'platform/api/tests/server-unit.test.ts')).toBe('integration');
  });

  it('integration: git init / sqlite / real tmpdir', () => {
    expect(tagTestType(`execSync('git init')`, 'x.test.ts')).toBe('integration');
    expect(tagTestType(`new Database(':memory:')`, 'x.test.ts')).toBe('integration');
    expect(tagTestType(`import sqlite3 from 'sqlite3'`, 'x.test.ts')).toBe('integration');
  });

  it('api: startTestApp / callTool / curl to a :33xx port', () => {
    expect(tagTestType(`const app = await startTestApp()`, 'x.test.ts')).toBe('api');
    expect(tagTestType(`await callTool('chorus_cards_add', {})`, 'x.test.ts')).toBe('api');
    expect(tagTestType(`fetch('http://localhost:3340/api/...')`, 'x.test.ts')).toBe('api');
  });

  it('security: _gate / _guard / _scrubber / approval surfaces', () => {
    expect(tagTestType(`expect(write_scrubber(secret)).toThrow()`, 'x.test.ts')).toBe('security');
    expect(tagTestType(`canonical_write_guard(path)`, 'x.test.ts')).toBe('security');
  });

  it('perf: Instant + elapsed assertion', () => {
    expect(tagTestType(`const t = Instant::now(); assert!(t.elapsed() < limit)`, 'x.rs')).toBe('perf');
  });

  it('ui: HTML / DOM / mermaid', () => {
    expect(tagTestType(`document.querySelector('.card')`, 'x.test.ts')).toBe('ui');
    expect(tagTestType(`render mermaid diagram`, 'x.bats')).toBe('ui');
  });

  it('unit: pure logic with jest.mock and no real-resource signal', () => {
    const content = `
      jest.mock('../src/db');
      const sum = add(1, 2);
    `;
    expect(tagTestType(content, 'platform/api/tests/add.test.ts')).toBe('unit');
  });

  it('MOCK-NEVER-PROMOTES: jest.mock of a real-resource module name, no real call → unit', () => {
    // The premise of the total order: mocking a module REMOVES the dependency.
    // A mocked module name must not match a real-resource signal.
    expect(tagTestType(`jest.mock('sqlite3'); const sum = add(1, 2);`, 'x.test.ts')).toBe('unit');
    expect(tagTestType(`jest.mock('child_process'); const sum = add(1, 2);`, 'x.test.ts')).toBe('unit');
    expect(tagTestType(`vi.mock('child_process')`, 'x.test.ts')).toBe('unit');
  });

  it('MOCK + REAL: mock one module but really use another → the real one wins', () => {
    expect(tagTestType(`jest.mock('sqlite3'); new Database(':memory:')`, 'x.test.ts')).toBe('integration');
  });

  it('PRECEDENCE: real-resource signals DOMINATE mock signals (mixed file)', () => {
    // mocks the db BUT also grabs a real tmpdir → the real resource wins.
    const content = `
      jest.mock('../src/db');
      const dir = mkdtempSync('/tmp/mixed-');
    `;
    expect(tagTestType(content, 'platform/api/tests/mixed.test.ts')).toBe('integration');
  });
});
