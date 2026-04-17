module.exports = {
  preset: 'ts-jest',
  testEnvironment: 'node',
  testMatch: ['**/tests/**/*.test.ts'],
  // ts-jest diagnostics off — type checking is tsc's job, not the test runner's.
  // Tests in this dir were written for default-jest (no strict TS) and use
  // `body.data` style access on `unknown`-typed `res.json()` returns.
  transform: {
    '^.+\\.tsx?$': ['ts-jest', { diagnostics: false }],
  },
  // Coverage: #2161 floor.
  coverageThreshold: {
    global: {
      branches: 60,
      functions: 75,
      lines: 80,
      statements: 80,
    },
  },
  collectCoverageFrom: ['src/**/*.ts', '!src/**/*.d.ts', '!src/**/*.test.ts'],
};
