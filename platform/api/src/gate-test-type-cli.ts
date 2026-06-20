#!/usr/bin/env node
// #3442 — test-type gate CLI. Takes test file paths as argv (the caller is
// responsible for diff-scoping — pre-commit passes staged files, CI passes the
// PR diff), runs the declaration gate, and exits non-zero if any are blocked.
import { readFileSync } from 'fs';
import { gateFiles, blockedFiles } from './gate-runner';

const paths = process.argv.slice(2);
// #3484 drift-cleanup: reading the file paths GIVEN as argv is this CLI's whole
// purpose (pre-commit passes staged files, CI passes the PR diff) — the path is
// non-literal by design, and the caller is trusted dev tooling, not user input.
// Justified suppression, not a baseline bump.
// eslint-disable-next-line security/detect-non-literal-fs-filename -- trusted-caller argv paths (#3442 gate CLI)
const results = gateFiles(paths, (p) => readFileSync(p, 'utf8'));
const blocked = blockedFiles(results);

if (blocked.length > 0) {
  process.stderr.write(`\n✗ test-type gate (#3442): ${blocked.length} file(s) blocked\n`);
  for (const b of blocked) {
    process.stderr.write(`  ${b.path}\n    ${b.result.reason}\n`);
  }
  process.stderr.write(
    '\nEach test file needs a header `// @test-type: <unit|integration|api|ui|perf|security|bdd|e2e>`.\n' +
    'If a content signal is fixture-data (a test ABOUT a domain carrying its words), justify the override:\n' +
    '  // @test-type: unit — signal:security is fixture-data\n',
  );
  process.exit(1);
}
process.exit(0);
