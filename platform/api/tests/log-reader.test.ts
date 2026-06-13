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
import { safeReadFile, readLogLines, readFileTail } from '../src/lib/log-reader';

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

  // #3406 — the event-loop freeze root: /context/spine readFileSync'd the whole
  // 535MB chorus.log on every request. readFileTail bounds that to the tail (the
  // recent events parseTailEvents wants), so the loop never blocks on a giant read.
  describe('readFileTail', () => {
    it('returns null for a missing file (does not throw)', () => {
      expect(readFileTail('/tmp/__definitely-not-a-real-file-3406__.log', 1024)).toBeNull();
    });

    it('returns the whole file when it is smaller than maxBytes', () => {
      const tmp = path.join(os.tmpdir(), `log-reader-tail-${process.pid}-${Date.now()}.log`);
      fs.writeFileSync(tmp, 'small\ncontent\n');
      try {
        expect(readFileTail(tmp, 1024)).toBe('small\ncontent\n');
      } finally {
        fs.unlinkSync(tmp);
      }
    });

    it('returns ONLY the last maxBytes when the file exceeds it (never the full read)', () => {
      const tmp = path.join(os.tmpdir(), `log-reader-tail-${process.pid}-${Date.now()}-2.log`);
      // a big file: filler + a marker at the very end (the "recent events")
      fs.writeFileSync(tmp, 'X'.repeat(5000) + 'TAIL_MARKER');
      try {
        const got = readFileTail(tmp, 100);
        expect(got).not.toBeNull();
        expect(got!.length).toBe(100);                // bounded, not 5011 bytes
        expect(got!.endsWith('TAIL_MARKER')).toBe(true); // it's the END of the file
      } finally {
        fs.unlinkSync(tmp);
      }
    });
  });
});
