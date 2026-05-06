/**
 * Unit tests for shared log-reader (#2126).
 *
 * AC: missing-log-file path test — verify safeReadFile / readLogLines
 * return null / [] (not throw) when the file does not exist. This is the
 * shape every handler depended on inline; making it a real shared
 * function with a real test prevents drift.
 */
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { safeReadFile, readLogLines } from '../src/lib/log-reader';

describe('log-reader', () => {
  describe('safeReadFile', () => {
    it('returns null for a missing file', () => {
      expect(safeReadFile('/tmp/__definitely-not-a-real-file-2126__.log')).toBeNull();
    });

    it('returns file contents when present', () => {
      const tmp = path.join(os.tmpdir(), `log-reader-test-${process.pid}-${Date.now()}.log`);
      fs.writeFileSync(tmp, 'line1\nline2\n');
      try {
        expect(safeReadFile(tmp)).toBe('line1\nline2\n');
      } finally {
        fs.unlinkSync(tmp);
      }
    });
  });

  describe('readLogLines', () => {
    it('returns [] for a missing file (does not throw)', () => {
      expect(readLogLines('/tmp/__definitely-not-a-real-file-2126__.log')).toEqual([]);
    });

    it('returns non-empty lines from a present file, dropping blanks', () => {
      const tmp = path.join(os.tmpdir(), `log-reader-test-${process.pid}-${Date.now()}-2.log`);
      fs.writeFileSync(tmp, 'one\ntwo\n\nthree\n');
      try {
        expect(readLogLines(tmp)).toEqual(['one', 'two', 'three']);
      } finally {
        fs.unlinkSync(tmp);
      }
    });
  });
});
