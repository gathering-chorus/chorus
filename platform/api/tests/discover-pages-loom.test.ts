// @test-type: unit — pure fs scanner against a tempdir; no services.
/**
 * #2485 Move 6 — discover-pages scanner extension for chorus/platform/api/public/loom/.
 *
 * Each <slug>.html in the loom dir maps to subdomain `loom-<slug>` if that
 * subdomain exists in the graph. Mirrors scanEjsViews / scanDocHtml shape.
 */
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { scanLoomHtml } from '../src/discover-pages-loom';

describe('scanLoomHtml', () => {
  let tmp: string;

  beforeEach(() => {
    tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'loom-scan-'));
  });

  afterEach(() => {
    fs.rmSync(tmp, { recursive: true, force: true });
  });

  test('returns empty array when directory does not exist', () => {
    expect(scanLoomHtml(path.join(tmp, 'missing'), new Set())).toEqual([]);
  });

  test('maps decisions.html to loom-decisions when subdomain is valid', () => {
    fs.writeFileSync(path.join(tmp, 'decisions.html'), '<html></html>');
    const valid = new Set(['loom-decisions']);
    const entries = scanLoomHtml(tmp, valid);
    expect(entries).toHaveLength(1);
    expect(entries[0].domainId).toBe('loom-decisions');
    expect(entries[0].route).toBe('/loom/decisions.html');
    expect(entries[0].pageType).toBe('loom');
    expect(entries[0].path).toContain('decisions.html');
  });

  test('skips files whose derived subdomain is not in the valid set', () => {
    fs.writeFileSync(path.join(tmp, 'principles-reference-impl.html'), '<html></html>');
    const valid = new Set(['loom-principles', 'loom-decisions']);
    const entries = scanLoomHtml(tmp, valid);
    expect(entries).toEqual([]);
  });

  test('emits one entry per matching html and ignores non-html files', () => {
    fs.writeFileSync(path.join(tmp, 'decisions.html'), 'x');
    fs.writeFileSync(path.join(tmp, 'principles.html'), 'x');
    fs.writeFileSync(path.join(tmp, 'principles.md'), 'not html');
    const valid = new Set(['loom-decisions', 'loom-principles']);
    const entries = scanLoomHtml(tmp, valid);
    const ids = entries.map((e) => e.domainId).sort();
    expect(ids).toEqual(['loom-decisions', 'loom-principles']);
  });

  test('uses chorus-relative path prefix in the path field', () => {
    fs.writeFileSync(path.join(tmp, 'decisions.html'), 'x');
    const entries = scanLoomHtml(tmp, new Set(['loom-decisions']));
    expect(entries[0].path).toBe('chorus/platform/api/public/loom/decisions.html');
  });
});
