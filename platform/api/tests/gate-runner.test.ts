// @test-type: unit — pure runner over the gate; file contents are FIXTURE INPUTS via an injected reader.
// #3442 — diff-scoped gate runner. Given a list of (already diff-filtered) test
// file paths, run the declaration gate over each and report which are blocked.
// The runner NEVER discovers files itself — it only sees the paths it's handed,
// which is what keeps the gate diff-scoped (it can never become a 404-file mandate).
import { gateFiles, blockedFiles, isTestFile } from '../src/gate-runner';

const reader = (files: Record<string, string>) => (p: string) => {
  if (!(p in files)) throw new Error(`missing ${p}`);
  return files[p];
};

describe('gate-runner', () => {
  it('only treats real test files as in-scope', () => {
    expect(isTestFile('platform/api/tests/foo.test.ts')).toBe(true);
    expect(isTestFile('scripts/check.bats')).toBe(true);
    expect(isTestFile('platform/api/src/server.ts')).toBe(false);
    expect(isTestFile('README.md')).toBe(false);
  });

  it('blocks an undeclared test file, passes a declared one', () => {
    const files = {
      'a.test.ts': '// @test-type: integration\nconst d = mkdtempSync("/tmp/x")',
      'b.test.ts': 'const x = 1; // no declaration',
    };
    const results = gateFiles(['a.test.ts', 'b.test.ts'], reader(files));
    const blocked = blockedFiles(results);
    expect(blocked.map((r) => r.path)).toEqual(['b.test.ts']);
  });

  it('a justified override is NOT blocked', () => {
    const files = {
      'c.test.ts': '// @test-type: unit — signal:security is fixture-data\nexpect(tag("write_scrubber(x)")).toBe("security")',
    };
    const blocked = blockedFiles(gateFiles(['c.test.ts'], reader(files)));
    expect(blocked).toEqual([]);
  });

  it('returns empty when handed no files (clean diff = no work)', () => {
    expect(gateFiles([], reader({}))).toEqual([]);
  });
});
