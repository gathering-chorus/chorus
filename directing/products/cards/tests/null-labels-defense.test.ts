/**
 * #2512 — `cards list` errored with "task.labels is not iterable" when
 * Vikunja returned `labels: null` (not `[]`) for tasks with no labels.
 * The type lied (`labels: VikunjaLabel[]`) and 4 iteration sites assumed
 * non-null.
 *
 * This test exercises the user-visible behavior: `client.list()` should
 * succeed when the board contains a task with `labels: null`. It must
 * fail before the fix (TypeError on iteration) and pass after.
 */
import { BoardClient } from '../src/client';
import { GATHERING } from '../src/config';
import type { VikunjaTask } from '../src/types';

describe('#2512 null labels defense', () => {
  test('client.list() does not crash when a task has labels: null', async () => {
    const client = new BoardClient('http://stub.invalid', 'fake-token', GATHERING);

    // Mock the two API touch points list() depends on.
    const taskWithNullLabels = {
      id: 999001,
      index: 9001,
      title: 'Card with null labels',
      description: '',
      done: false,
      created: '2026-04-27',
      updated: '2026-04-27',
      labels: null,                    // ← the bug condition
      project_id: 1,
    } as unknown as VikunjaTask;

    jest.spyOn(client as unknown as { fetchAllTasks: () => Promise<VikunjaTask[]> }, 'fetchAllTasks')
      .mockResolvedValue([taskWithNullLabels]);
    jest.spyOn(client as unknown as { fetchBucketMapFromDB: () => Map<number, string> }, 'fetchBucketMapFromDB')
      .mockReturnValue(new Map());

    // Must not throw "task.labels is not iterable"
    const tasks = await client.list();
    expect(tasks).toHaveLength(1);
    expect(tasks[0].title).toBe('Card with null labels');
    // Empty domains/owner/priority is the expected normalized output for null labels
    expect(tasks[0].domains).toEqual([]);
    expect(tasks[0].owner).toBe('');
    expect(tasks[0].priority).toBe('');
  });
});
