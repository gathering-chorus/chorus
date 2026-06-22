// #3559 — platform/api jest is split into two named projects so the nightly can
// judge each tier correctly and stop false-redding:
//
//   hermetic    — runs ALWAYS, counts toward the nightly's red. No live stack:
//                 handlers are imported and called with injected deps, or driven
//                 through the in-process harness (tests/lib/test-app.ts) on an
//                 ephemeral port. A stack-down nightly leaves these GREEN.
//   integration — real HTTP / DB / live Fuseki / :33xx services. Stack-gated:
//                 opt-in via RUN_INTEGRATION=true, which the nightly sets only
//                 when the stack is up (#3557 _stack_up gate). Stack down → this
//                 project is not even constructed, so it SKIPS rather than failing
//                 for-no-stack.
//
// Tier is decided by the `*.integration.test.ts` filename suffix — the #2524
// audit convention (post-#2523). Default `jest` runs hermetic only; the nightly
// keeps its current hermetic-only behavior with no runner change, and turns the
// integration project on by exporting RUN_INTEGRATION=true when the stack is up.

// Shared per-project settings. With a `projects` config these (preset,
// testEnvironment, transform) must live ON each project — top-level copies are
// ignored by jest. maxWorkers / coverage / reporters stay global (below).
const base = {
  preset: 'ts-jest',
  testEnvironment: 'node',
  // ts-jest diagnostics off — type checking is tsc's job, not the test runner's.
  // Tests here were written for default-jest (no strict TS) and use `body.data`
  // style access on `unknown`-typed `res.json()` returns.
  // #2495: tsconfig.json moves to module=node16 so tsc honors the SDK's exports
  // field. ts-jest stays on commonjs so dynamic `await import()` calls in tests
  // (e.g. test-app.ts) transpile to require(), avoiding --experimental-vm-modules.
  transform: {
    '^.+\\.tsx?$': ['ts-jest', {
      diagnostics: false,
      tsconfig: { module: 'commonjs', moduleResolution: 'node', esModuleInterop: true },
    }],
  },
};

const hermeticProject = {
  ...base,
  displayName: 'hermetic',
  testMatch: ['**/tests/**/*.test.ts'],
  testPathIgnorePatterns: [
    '/node_modules/',
    // integration tier — excluded from hermetic; runs in the integration project.
    '\\.integration\\.test\\.ts$',
    // #3559: sessions.test.ts was excluded here pending review (audit-flagged
    // "review, not renamed"). Review done — it's pure-injected-deps (@test-type:
    // unit), touches no live service, so it belongs in the HERMETIC project.
    // Silas confirmed the promotion. No longer excluded.
  ],
};

const integrationProject = {
  ...base,
  displayName: 'integration',
  testMatch: [
    '**/tests/**/*.integration.test.ts',
  ],
  testPathIgnorePatterns: ['/node_modules/'],
};

// Stack gate (#3557 parity). The nightly exports RUN_INTEGRATION=true only when
// the live stack is up; otherwise the integration project is never built, so its
// tests SKIP (no false red). `npm run test:integration` / `test:coverage` set it.
const RUN_INTEGRATION = process.env.RUN_INTEGRATION === 'true';

module.exports = {
  // maxWorkers history:
  //   6734fe2d (#2161): pinned to 1 — parallel workers hit shared :3340, flakes.
  //   #2173 AC4: tests moved off live :3340 onto the in-process harness
  //     (tests/lib/test-app.ts); each worker gets an ephemeral port from
  //     app.listen(0). 101s serial → 38s at 8 workers across 393 passing tests.
  //     Lifted to 50%; a handful still hit real Fuseki/SQLite with state-mutating
  //     writes (those get mocked at handler seams as decomposition lands). If
  //     flakes return, lower, don't re-pin to 1.
  maxWorkers: '50%',

  projects: RUN_INTEGRATION
    ? [hermeticProject, integrationProject]
    : [hermeticProject],

  // Coverage is global (applies to the --coverage aggregate across projects).
  // Baseline 2026-04-17: 0% across all metrics — the original suite was all HTTP
  // integration against the live chorus-api on :3340, a separate Node process
  // (LaunchAgent) that jest can't instrument. Floor kept at 0 to not block the
  // pipeline; real coverage requires importing handlers into tests (in-process
  // harness) or subprocess instrumentation (nyc/c8) — separate scope from #2167.
  coverageThreshold: {
    global: {
      branches: 0,
      functions: 0,
      lines: 0,
      statements: 0,
    },
    'src/patterns-summary.ts': {
      branches: 60, functions: 75, lines: 80, statements: 80,
    },
    'src/session-replay.ts': {
      branches: 60, functions: 75, lines: 80, statements: 80,
    },
    'src/jeff-summary.ts': {
      branches: 60, functions: 75, lines: 80, statements: 80,
    },
    'src/hooks-summary.ts': {
      branches: 60, functions: 75, lines: 80, statements: 80,
    },
    'src/fitness-summary.ts': {
      branches: 60, functions: 75, lines: 80, statements: 80,
    },
    'src/cost-summary.ts': {
      branches: 60, functions: 75, lines: 80, statements: 80,
    },
    'src/quality-summary.ts': {
      branches: 60, functions: 75, lines: 80, statements: 80,
    },
    // server.ts — 7225 lines, 136 route handlers, ~9% from the in-process smoke
    // suite (#2167). Per-file threshold at the current floor so regressions trip
    // the build; raising it is its own multi-file conversion effort.
    'src/server.ts': {
      branches: 1, functions: 1, lines: 5, statements: 5,
    },
  },
  collectCoverageFrom: ['src/**/*.ts', '!src/**/*.d.ts', '!src/**/*.test.ts'],

  // Quiet reporter by default (#2225). Cuts TTY progress chatter; failures still
  // surface via summary with summaryThreshold: 0. JEST_VERBOSE=true → default.
  reporters: process.env.JEST_VERBOSE === 'true'
    ? ['default']
    : [['summary', { summaryThreshold: 0 }]],
};
