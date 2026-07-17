// @test-type: integration — reads the real backfilled test files from disk.
//
// #3442 backfill guard: the *-unit.test.ts files that touch a real tmpdir
// were mislabeled "unit" by the old path-only heuristic. They now carry an
// explicit `@test-type: integration` declaration. This test reads each file
// and asserts the declaration is present AND the gate passes (declaration
// matches content signals) — so removing a declaration, or a file drifting
// back below its real signal, goes red here.
// (#3657 retired quality-summary-unit.test.ts with the filesystem scanner it
// covered — the tests-domain projection has its own hermetic suite — so the
// original 9 is now 8.)
import { readFileSync } from 'fs';
import { join } from 'path';
import { gateTestType } from '../src/gate-test-type';

const HERE = __dirname;
const BACKFILLED = [
  'cost-summary-unit.test.ts',
  'fitness-summary-unit.test.ts',
  'server-unit.test.ts',
  'jeff-summary-unit.test.ts',
  'hooks-summary-unit.test.ts',
  'session-replay-unit.test.ts',
].map((f) => join(HERE, f));

const CLEARING = [
  'server-unit.test.ts',
  'session-tailer-unit.test.ts',
].map((f) => join(HERE, '../../../directing/clearing/tests', f));

describe('#3442 backfill — the real-fs *-unit files are declared integration', () => {
  it('every backfilled file declares integration and passes the gate', () => {
    const files = [...BACKFILLED, ...CLEARING];
    expect(files).toHaveLength(8);
    for (const file of files) {
      const content = readFileSync(file, 'utf8');
      const result = gateTestType(content, file);
      expect(result.declared).toBe('integration');
      expect(result.signalled).toBe('integration');
      expect(result.ok).toBe(true);
    }
  });
});
