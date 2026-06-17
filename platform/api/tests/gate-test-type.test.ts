// @test-type: unit — pure tests of the gate logic; the @test-type tokens below are FIXTURE INPUTS, not this file's declaration.
// #3442 — test-type declaration gate.
// A test DECLARES its type (// @test-type: X); the gate blocks a new/changed
// test with NO declaration, or whose declaration CONTRADICTS its content
// signals (the drift: declaring "unit" on a file that touches real fs).
import { gateTestType } from '../src/gate-test-type';

describe('gateTestType', () => {
  it('blocks a file with NO declaration', () => {
    const r = gateTestType(`const x = 1;`, 'tests/foo.test.ts');
    expect(r.ok).toBe(false);
    expect(r.reason).toMatch(/no .*declaration/i);
  });

  it('THE DRIFT: blocks declared "unit" when signals say integration', () => {
    const content = `
      // @test-type: unit
      import { mkdtempSync } from 'fs';
      const dir = mkdtempSync('/tmp/x-');
    `;
    const r = gateTestType(content, 'tests/server-unit.test.ts');
    expect(r.ok).toBe(false);
    expect(r.reason).toMatch(/unit/);
    expect(r.reason).toMatch(/integration/);
  });

  it('passes when declaration matches the signalled type', () => {
    const content = `
      // @test-type: integration
      const dir = mkdtempSync('/tmp/x-');
    `;
    const r = gateTestType(content, 'tests/real-fs.test.ts');
    expect(r.ok).toBe(true);
    expect(r.declared).toBe('integration');
    expect(r.signalled).toBe('integration');
  });

  it('passes a pure unit test that declares unit', () => {
    const content = `
      // @test-type: unit
      jest.mock('../src/db');
      const sum = add(1, 2);
    `;
    const r = gateTestType(content, 'tests/add.test.ts');
    expect(r.ok).toBe(true);
  });

  it('DECLARATION-IN-DATA: a @test-type marker BELOW real code is fixture data, not honored', () => {
    // The marker is past the leading comment block (real code came first), so it
    // is NOT the file's declaration — it's a fixture string. Result: undeclared.
    const content = `const cases = [\n  '// @test-type: api',\n];`;
    const r = gateTestType(content, 'tests/parser.test.ts');
    expect(r.declared).toBeNull();
    expect(r.ok).toBe(false);
  });

  it('PROSE PLACEHOLDER: a non-vocab @test-type mention in the header is not a declaration', () => {
    // e.g. a doc comment "// see @test-type: X for usage" — X is not vocab.
    const content = `// docs mention @test-type: X as a placeholder\nconst sum = add(1, 2);`;
    const r = gateTestType(content, 'tests/foo.test.ts');
    expect(r.declared).toBeNull();
  });

  it('JUSTIFIED OVERRIDE: declared lighter than a fixture-data signal passes WITH a reason', () => {
    // a test ABOUT security carries 'write_scrubber' as fixture data → signals
    // security, but it is really unit. The justification makes the override ok.
    const content = `// @test-type: unit — signal:security is fixture-data\nexpect(tag('write_scrubber(x)')).toBe('security');`;
    const r = gateTestType(content, 'tests/tagger.test.ts');
    expect(r.ok).toBe(true);
    expect(r.override).toBe(true);
    expect(r.declared).toBe('unit');
    expect(r.signalled).toBe('security');
  });

  it('UNJUSTIFIED under-claim is still blocked (no silent override)', () => {
    // same mismatch, but NO justification on the declaration line → blocked.
    const content = `// @test-type: unit\nexpect(tag('write_scrubber(x)')).toBe('security');`;
    const r = gateTestType(content, 'tests/tagger.test.ts');
    expect(r.ok).toBe(false);
    expect(r.reason).toMatch(/justify the override/i);
  });
});
