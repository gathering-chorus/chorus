// @test-type: unit — writes only to its own mkdtemp dir via CARDS_CACHE_DIR; the fs signal is fixture data, no live service or shared state
/**
 * #4010 — a cache key without identity cannot separate authorized from remembered.
 *
 * Kade found this clearing a nightly red, 2026-08-26: `list()` → `fetchAllTasks()`
 * read `fileTaskCache(projectId)` BEFORE any API call, and the key was the
 * project id alone. A client holding a bad credential got the whole board out of
 * the cache an authorized client had filled — never contacting Vikunja, never
 * seeing the 401 Vikunja was willing to give. Vikunja was fine; the client was
 * answering from memory and calling it authorization.
 *
 * These tests grade the KEY, not the happy path, because the happy path passed
 * throughout.
 */
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { fileTaskCache, cacheIdentity } from '../src/task-cache';

const TASKS = [{ id: 1, index: 1, title: 'a card' }] as never;

describe('#4010 cache identity', () => {
  let dir: string;
  beforeEach(() => {
    dir = fs.mkdtempSync(path.join(os.tmpdir(), 'cards-cache-4010-'));
    process.env.CARDS_CACHE_DIR = dir;
    delete process.env.CARDS_CACHE_DISABLE;
  });
  afterEach(() => {
    delete process.env.CARDS_CACHE_DIR;
    fs.rmSync(dir, { recursive: true, force: true });
  });

  test('the same credential reads its own write', () => {
    const me = fileTaskCache(2, cacheIdentity('good-token'));
    me.write(TASKS);
    expect(me.read()).not.toBeNull();
  });

  // THE DEFECT. Without identity in the key this returns the other caller's
  // tasks, which is exactly what shipped.
  test('a DIFFERENT credential does not read the first one\'s cache', () => {
    fileTaskCache(2, cacheIdentity('good-token')).write(TASKS);
    const other = fileTaskCache(2, cacheIdentity('bad-token'));
    expect(other.read()).toBeNull();
  });

  test('no credential keys as anon, not as the authorized pool', () => {
    fileTaskCache(2, cacheIdentity('good-token')).write(TASKS);
    expect(fileTaskCache(2, cacheIdentity(undefined)).read()).toBeNull();
    expect(cacheIdentity(undefined)).toBe('anon');
  });

  // NEGATIVE PROOF — the check must be capable of failing. Keyed the OLD way
  // (identity omitted), the two callers DO share a cache: this asserts the
  // vulnerable shape is still reachable and still wrong, so the test above is
  // measuring the fix rather than an accident of tmpdir naming.
  test('the OLD key shape still shares across callers — the bug, reproduced', () => {
    fileTaskCache(2).write(TASKS);
    expect(fileTaskCache(2).read()).not.toBeNull();
  });

  test('the token itself never appears in the cache path or contents', () => {
    const token = 'super-secret-token-value';
    fileTaskCache(2, cacheIdentity(token)).write(TASKS);
    const files = fs.readdirSync(dir);
    expect(files.join(' ')).not.toContain(token);
    for (const f of files) {
      expect(fs.readFileSync(path.join(dir, f), 'utf8')).not.toContain(token);
    }
    expect(cacheIdentity(token)).toHaveLength(12);
  });
});
