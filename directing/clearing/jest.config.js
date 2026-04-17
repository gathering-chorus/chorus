module.exports = {
  preset: 'ts-jest',
  testEnvironment: 'node',
  testMatch: ['**/tests/**/*.test.ts'],
  moduleFileExtensions: ['ts', 'js', 'json'],
  // Coverage floors — #2167. Target: 60/75/80 (gathering baseline).
  //
  // Per-file thresholds lock modules as they hit 80% — regression on a
  // tested module fails the build, untested modules don't block. Global
  // threshold stays low until all src/ files are covered.
  //
  // Order: tailer (phase 1), transcript, participants → router, tiles,
  // chat, session-tailer (phase 2) → server.ts factory refactor (phase 3).
  coverageThreshold: {
    global: { branches: 0, functions: 0, lines: 0, statements: 0 },
    'src/tailer.ts': { branches: 60, functions: 75, lines: 80, statements: 80 },
  },
  collectCoverageFrom: ['src/**/*.ts', '!src/**/*.d.ts', '!src/**/*.test.ts'],
};
