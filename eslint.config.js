/* eslint-env node */
// #2284: Chorus root ESLint config — parity with jeff-bridwell-personal-site/eslint.config.js.
// All package src/ and tests/ across platform/ and directing/ are linted by this single config.

const js = require('@eslint/js');
const tsParser = require('@typescript-eslint/parser');
const tsPlugin = require('@typescript-eslint/eslint-plugin');
const security = require('eslint-plugin-security');
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
      parserOptions: { ecmaVersion: 'latest', sourceType: 'module' },
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
      'security/detect-non-literal-fs-filename': 'off',
      'security/detect-object-injection': 'off',
      'no-eval': 'error',
      'no-implied-eval': 'error',
      'no-new-func': 'error',
      '@typescript-eslint/explicit-function-return-type': 'off',
      '@typescript-eslint/no-explicit-any': 'off',
      '@typescript-eslint/no-unused-vars': ['error', {
        'argsIgnorePattern': '^_',
        'varsIgnorePattern': '^_',
      }],
      '@typescript-eslint/no-require-imports': 'off',
      // TypeScript handles type/import resolution better than ESLint's no-undef.
      // Disabling prevents false positives on TS built-ins (NodeJS, BufferEncoding, RequestInit).
      'no-undef': 'off',
      'complexity': ['error', { max: 20 }],
      // #2288 baseline: raised from 4/80 to 7/274 for the 29-site suppression
      // wave. All 29 `complexity` suppressions are now refactored; max-depth
      // and max-lines-per-function at tight budgets still surface 23 separate
      // pre-existing sites. Tighten in a follow-on wave.
      'max-depth': ['warn', { max: 7 }],
      'max-lines-per-function': ['warn', { max: 274, skipBlankLines: true, skipComments: true }],
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
    rules: {
      '@typescript-eslint/no-unused-vars': 'off',
      '@typescript-eslint/ban-ts-comment': 'off',
      '@typescript-eslint/ban-types': 'off',
      'max-lines-per-function': ['warn', { max: 1500, skipBlankLines: true, skipComments: true }],
      'complexity': 'off',
    },
  },
];
