/**
 * chorus-seed-media handler — unit tests (#2189).
 *
 * resolveSeedMedia returns the filePath to send (200) or an error body
 * (400 / 404). Tests verify:
 *   - valid name + file exists → 200 with joined path
 *   - path traversal '..' → 400
 *   - '/' in name → 400
 *   - empty name → 400
 *   - valid name + file missing → 404
 *   - allowed characters: letters, digits, dot, dash, underscore
 */
import { resolveSeedMedia } from '../../src/handlers/chorus-seed-media';

const BASE = '/tmp/seed-media';

describe('resolveSeedMedia (#2189 /api/chorus/seed-media/:filename)', () => {
  test('valid filename + file exists → 200 + joined filePath', () => {
    const r = resolveSeedMedia('cover.png', {
      baseDir: BASE,
      exists: () => true,
    });
    expect(r).toEqual({ status: 200, filePath: '/tmp/seed-media/cover.png' });
  });

  test("path traversal with '..' → 400", () => {
    const r = resolveSeedMedia('../etc/passwd', {
      baseDir: BASE,
      exists: () => true,
    });
    expect(r).toEqual({ status: 400, body: { error: 'Invalid filename' } });
  });

  test("'/' in filename → 400", () => {
    const r = resolveSeedMedia('sub/file.png', {
      baseDir: BASE,
      exists: () => true,
    });
    expect(r).toEqual({ status: 400, body: { error: 'Invalid filename' } });
  });

  test('empty filename → 400', () => {
    const r = resolveSeedMedia('', { baseDir: BASE, exists: () => true });
    expect(r).toEqual({ status: 400, body: { error: 'Invalid filename' } });
  });

  test('valid name + file missing → 404', () => {
    const r = resolveSeedMedia('missing.png', {
      baseDir: BASE,
      exists: () => false,
    });
    expect(r).toEqual({ status: 404, body: { error: 'Media not found' } });
  });

  test('accepts letters, digits, dot, dash, underscore', () => {
    const r = resolveSeedMedia('seed_42.jpg-v1', {
      baseDir: BASE,
      exists: () => true,
    });
    expect(r.status).toBe(200);
  });

  test('rejects null byte', () => {
    const r = resolveSeedMedia('ok.png\0.jpg', {
      baseDir: BASE,
      exists: () => true,
    });
    expect(r.status).toBe(400);
  });
});
