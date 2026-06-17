// #3442 — content-signal test-type tagger.
//
// Replaces the path-only heuristic (discover-tests.ts:classifyTestType) that
// mislabeled 7 *-unit.test.ts files (real fs → wrongly "unit"). This reads
// MECHANICAL CONTENT SIGNALS, never the path or the filename.
//
// DETERMINISM CONTRACT (#3442, navigated with Silas):
//   - A mock is NEVER a positive signal. Mocking REMOVES a real dependency, so
//     it can only leave a file at the floor (unit) — it never promotes.
//   - Only REAL resources promote. The order is TOTAL with no ambiguous ties:
//     security > perf > ui > api > integration > unit(floor).
//   - The tagger scans top-down and returns the FIRST matching real signal.
//     Every file resolves to exactly one type — that total order IS the
//     determinism this card exists to create.
//
// FOLLOW-ON (not this card): the rules below are config-in-code. The eventual
// lift is rules → declared data, once test-type proves the declare+gate pattern.

export type TestType = 'security' | 'perf' | 'ui' | 'api' | 'integration' | 'unit';

// Ordered highest-precedence first. First REAL-signal match wins; unit is the floor.
const SIGNALS: Array<{ type: TestType; pattern: RegExp }> = [
  // security: gate / guard / scrubber / approval surfaces
  { type: 'security', pattern: /_gate\b|_guard\b|_scrubber\b|\bapproval\b|\bwrite_scrubber\b/ },
  // perf: timed assertions
  { type: 'perf', pattern: /Instant::now|\.elapsed\(\)|performance\.now\b/ },
  // ui: HTML / DOM / mermaid rendering
  { type: 'ui', pattern: /document\.|querySelector|getElementById|\.innerHTML|\bmermaid\b/ },
  // api: in-process app / tool harness / a real HTTP call to a chorus :33xx
  // service port. Require the URL scheme (or a named harness call) so a bare
  // host:port appearing as test FIXTURE DATA (e.g. 'curl localhost:3340/x' in a
  // command-classifier's input table) does not false-positive as an api test.
  { type: 'api', pattern: /startTestApp|callTool|https?:\/\/localhost:33\d\d|https?:\/\/127\.0\.0\.1:33\d\d/ },
  // integration: real fs / git / db / subprocess
  { type: 'integration', pattern: /mkdtemp|git init|\bsqlite\w*|new Database|Command::new|execSync|spawnSync|child_process/ },
];

/**
 * Classify a test file by its content signals. Pure: same (content, relPath)
 * always yields the same type. relPath is accepted for future path-aware rules
 * (e.g. .bats/.feature → bdd as a pre-layer) but content signals are
 * authoritative — the path/filename never overrides a real signal.
 */
export function tagTestType(content: string, _relPath: string): TestType {
  // Strip mock declarations before scanning. A mock REMOVES a real dependency,
  // so a mocked module name (jest.mock('sqlite3')) must never match a
  // real-resource signal — otherwise mocking could promote, and the total order
  // stops being deterministic. Only real, unmocked usage promotes.
  const real = content.replace(/\b(?:jest|vi)\.mock\s*\([^)]*\)/g, '');
  for (const { type, pattern } of SIGNALS) {
    if (pattern.test(real)) return type;
  }
  return 'unit';
}
