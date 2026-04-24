/* eslint-env node */
// #2284: Chorus root ESLint config — parity with jeff-bridwell-personal-site/eslint.config.js.
// All package src/ and tests/ across platform/ and directing/ are linted by this single config.

const js = require('@eslint/js');
const tsParser = require('@typescript-eslint/parser');
const tsPlugin = require('@typescript-eslint/eslint-plugin');
const security = require('eslint-plugin-security');
const jestPlugin = require('eslint-plugin-jest');
const globals = require('globals');

module.exports = [
  {
    ignores: [
      '**/dist/**',
      '**/node_modules/**',
      '**/coverage/**',
      '**/*.d.ts',
      'roles/**',
      'designing/**',
      'platform/services/**',
      'platform/scripts/**',
      'platform/state/**',
      'platform/tests/fixtures/**',
    ],
  },
  js.configs.recommended,
  {
    files: ['platform/**/src/**/*.ts', 'directing/**/src/**/*.ts'],
    languageOptions: {
      parser: tsParser,
      parserOptions: {
        ecmaVersion: 'latest',
        sourceType: 'module',
        projectService: true,
        tsconfigRootDir: __dirname,
      },
      globals: {
        ...globals.node,
        ...globals.es2021,
        ...globals.jest,
        fetch: 'readonly',
        AbortSignal: 'readonly',
        Headers: 'readonly',
        Request: 'readonly',
        Response: 'readonly',
        FormData: 'readonly',
      },
    },
    plugins: {
      '@typescript-eslint': tsPlugin,
      'security': security,
    },
    rules: {
      ...tsPlugin.configs.recommended.rules,
      ...security.configs.recommended.rules,
      'no-eval': 'error',
      'no-implied-eval': 'error',
      'no-new-func': 'error',
      '@typescript-eslint/explicit-function-return-type': 'off',
      '@typescript-eslint/no-explicit-any': 'error',
      '@typescript-eslint/no-floating-promises': 'error',
      // checksVoidReturn.arguments: false — Express 4 @types say route handlers
      // must return void, but Express accepts Promise-returning handlers fine
      // (Express 5 types this natively). Without this override, every async
      // route lights up as a "promise in void slot" false positive. We keep
      // `assignments`/`properties` checks on — those catch real bugs where a
      // returned Promise is silently ignored (#2463 wave 2).
      '@typescript-eslint/no-misused-promises': ['error', {
        checksVoidReturn: { arguments: false, attributes: false },
      }],
      '@typescript-eslint/await-thenable': 'error',
      '@typescript-eslint/require-await': 'error',
      '@typescript-eslint/no-base-to-string': 'error',
      '@typescript-eslint/no-unnecessary-condition': 'error',
      '@typescript-eslint/no-unused-vars': ['error', {
        'argsIgnorePattern': '^_',
        'varsIgnorePattern': '^_',
      }],
      '@typescript-eslint/no-require-imports': 'off',
      // TypeScript handles type/import resolution better than ESLint's no-undef.
      // Disabling prevents false positives on TS built-ins (NodeJS, BufferEncoding, RequestInit).
      'no-undef': 'off',
      'complexity': ['error', { max: 15 }],
      'max-depth': ['error', { max: 4 }],
      'max-lines-per-function': ['error', { max: 80, skipBlankLines: true, skipComments: true }],
      'no-console': 'off',
      'no-debugger': 'error',
      'no-duplicate-imports': 'error',
      'no-unused-vars': 'off',
      'prefer-const': 'error',
      'quotes': ['error', 'single', { 'avoidEscape': true }],
      'semi': ['error', 'always'],
    },
  },
  {
    files: ['**/tests/**/*.ts'],
    languageOptions: {
      parser: tsParser,
      parserOptions: { ecmaVersion: 'latest', sourceType: 'module' },
      globals: {
        ...globals.node,
        ...globals.es2021,
        ...globals.jest,
        fetch: 'readonly',
      },
    },
    plugins: {
      'jest': jestPlugin,
    },
    settings: {
      jest: { version: 29 },
    },
    rules: {
      ...jestPlugin.configs['flat/recommended'].rules,
      'jest/expect-expect': 'error',
      'jest/no-disabled-tests': 'error',
      'jest/no-focused-tests': 'error',
      'jest/valid-expect': 'error',
      '@typescript-eslint/no-unused-vars': 'off',
      '@typescript-eslint/ban-ts-comment': 'off',
      '@typescript-eslint/ban-types': 'off',
      'no-unused-vars': 'off',
      'max-lines-per-function': ['warn', { max: 1500, skipBlankLines: true, skipComments: true }],
      'complexity': 'off',
    },
  },
];
