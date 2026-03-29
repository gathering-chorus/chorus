/**
 * Integration tests — run against live Vikunja instance.
 * Read-only: no mutations, just verify we can talk to the API.
 *
 * Skip if VIKUNJA_URL is not reachable.
 */
import { BoardClient } from '../src/client';
import { GATHERING, SELF, loadEnv } from '../src/config';

let env: { url: string; token: string };
let canConnect = false;

beforeAll(async () => {
  try {
    env = loadEnv();
    // Quick connectivity check
    const client = new BoardClient(env.url, env.token, GATHERING);
    const tasks = await client.list();
    canConnect = Array.isArray(tasks);
  } catch {
    canConnect = false;
  }
});

function skipIfNoConnection() {
  if (!canConnect) {
    return true;
  }
  return false;
}

describe('Integration: Gathering board', () => {
  test('list returns tasks', async () => {
    if (skipIfNoConnection()) return;
    const client = new BoardClient(env.url, env.token, GATHERING);
    const tasks = await client.list();
    expect(tasks.length).toBeGreaterThan(0);
  });

  test('listGrouped returns map with status keys', async () => {
    if (skipIfNoConnection()) return;
    const client = new BoardClient(env.url, env.token, GATHERING);
    const grouped = await client.listGrouped();
    expect(grouped instanceof Map).toBe(true);
    // At minimum we expect some Done items to exist
    const allStatuses = Array.from(grouped.keys());
    expect(allStatuses.length).toBeGreaterThan(0);
  });

  test('mine returns tasks for wren', async () => {
    if (skipIfNoConnection()) return;
    const client = new BoardClient(env.url, env.token, GATHERING);
    const tasks = await client.mine('wren');
    expect(Array.isArray(tasks)).toBe(true);
    // Wren should have items on the Gathering board
    for (const t of tasks) {
      expect(t.owner.toLowerCase()).toBe('wren');
    }
  });

  test('snapshot returns valid structure', async () => {
    if (skipIfNoConnection()) return;
    const client = new BoardClient(env.url, env.token, GATHERING);
    const snap = await client.snapshot();
    expect(snap.board).toBe('gathering');
    expect(snap.timestamp).toBeDefined();
    expect(snap.tasks.length).toBeGreaterThan(0);
    // Every task has required fields
    for (const t of snap.tasks) {
      expect(t.index).toBeDefined();
      expect(t.title).toBeDefined();
      expect(t.status).toBeDefined();
      expect(typeof t.done).toBe('boolean');
    }
  });
});

describe('Integration: Self board', () => {
  test('list returns tasks', async () => {
    if (skipIfNoConnection()) return;
    const client = new BoardClient(env.url, env.token, SELF);
    const tasks = await client.list();
    expect(Array.isArray(tasks)).toBe(true);
  });

  test('view returns task details', async () => {
    if (skipIfNoConnection()) return;
    const client = new BoardClient(env.url, env.token, SELF);
    const tasks = await client.list();
    if (tasks.length === 0) return;
    // View the first task
    const task = await client.view(tasks[0].index);
    expect(task.title).toBe(tasks[0].title);
    expect(task.status).toBeDefined();
  });
});

describe('Integration: Error handling', () => {
  test('bad token returns auth error', async () => {
    if (skipIfNoConnection()) return;
    const client = new BoardClient(env.url, 'invalid-token', GATHERING);
    await expect(client.list()).rejects.toThrow();
  });

  test('invalid index throws', async () => {
    if (skipIfNoConnection()) return;
    const client = new BoardClient(env.url, env.token, GATHERING);
    await expect(client.resolveIndex(99999)).rejects.toThrow('No task #99999');
  });
});
