// @domain: messages

module.exports = {
  preset: 'ts-jest',
  testEnvironment: 'node',
  testMatch: ['**/tests/**/*.test.ts'],
  moduleFileExtensions: ['ts', 'js', 'json'],
  transform: {
    '^.+\\.tsx?$': ['ts-jest', { tsconfig: { esModuleInterop: true, skipLibCheck: true } }],
  },
  transformIgnorePatterns: ['node_modules/(?!@noble)/'],
  // #4966 — tests stay on this machine: one guard on fetch, shared with the v1 package
  setupFiles: ['<rootDir>/../../platform/tests/fixtures/stay-local-4966.setup.ts'],
};
