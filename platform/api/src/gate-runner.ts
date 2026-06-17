// #3442 — diff-scoped gate runner.
//
// Runs the test-type declaration gate over a list of ALREADY-DIFF-FILTERED test
// file paths. The runner never discovers files on its own — callers (pre-commit,
// CI) hand it exactly the changed test files. That is what keeps the gate
// diff-scoped: it can only ever block files in the current change, never demand
// the whole 404-file corpus declare at once.
import { gateTestType, type GateResult } from './gate-test-type';

const TEST_FILE_RE = /\.(test|spec)\.(ts|js)$|\.bats$|\.feature$/i;

export function isTestFile(path: string): boolean {
  return TEST_FILE_RE.test(path);
}

export interface GatedFile {
  path: string;
  result: GateResult;
}

export function gateFiles(paths: string[], readFile: (p: string) => string): GatedFile[] {
  return paths
    .filter(isTestFile)
    .map((path) => ({ path, result: gateTestType(readFile(path), path) }));
}

export function blockedFiles(results: GatedFile[]): GatedFile[] {
  return results.filter((r) => !r.result.ok);
}
