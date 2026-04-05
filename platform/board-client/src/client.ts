import * as https from 'http';
import { VikunjaTask, VikunjaBucket, BoardConfig, BoardTask } from './types';
import { LABELS, resolveBucket } from './config';

export class BoardClient {
  private url: string;
  private token: string;
  private board: BoardConfig;

  constructor(url: string, token: string, board: BoardConfig) {
    this.url = url.replace(/\/$/, '');
    this.token = token;
    this.board = board;
  }

  get boardName(): string {
    return this.board.name;
  }

  get config(): BoardConfig {
    return this.board;
  }

  // ── API layer ──

  private async api<T>(method: string, endpoint: string, body?: object): Promise<T> {
    const url = new URL(`/api/v1${endpoint}`, this.url);

    return new Promise((resolve, reject) => {
      const options = {
        method,
        hostname: url.hostname,
        port: url.port,
        path: url.pathname + url.search,
        headers: {
          'Authorization': `Bearer ${this.token}`,
          'Content-Type': 'application/json',
        },
      };

      const req = https.request(options, (res) => {
        let data = '';
        res.on('data', (chunk) => data += chunk);
        res.on('end', () => {
          if (res.statusCode && res.statusCode >= 400) {
            reject(new Error(`API ${method} ${endpoint}: ${res.statusCode} ${data}`));
            return;
          }
          try {
            resolve(JSON.parse(data) as T);
          } catch {
            resolve(data as unknown as T);
          }
        });
      });

      req.on('error', reject);
      if (body) req.write(JSON.stringify(body));
      req.end();
    });
  }

  // ── Task index resolution ──

  private taskMap: Map<number, number> | null = null;

  async buildTaskMap(): Promise<Map<number, number>> {
    if (this.taskMap) return this.taskMap;
    const buckets = await this.fetchBuckets();
    this.taskMap = new Map();
    for (const bucket of buckets) {
      for (const task of bucket.tasks || []) {
        this.taskMap.set(task.index ?? task.id, task.id);
      }
    }
    return this.taskMap;
  }

  /** Clear cached task map (call after mutations) */
  clearCache(): void {
    this.taskMap = null;
  }

  async resolveIndex(index: number): Promise<number> {
    const map = await this.buildTaskMap();
    const id = map.get(index);
    if (id !== undefined) return id;
    // Bucket query misses old Done tasks — fall back to full project scan
    const allTasks = await this.fetchAllTasks();
    for (const task of allTasks) {
      const idx = (task as any).index ?? task.id;
      if (!map.has(idx)) map.set(idx, task.id);
    }
    const fullId = map.get(index);
    if (fullId === undefined) {
      throw new Error(`No task #${index} on ${this.board.name} board`);
    }
    return fullId;
  }

  // ── Core operations ──

  async fetchBuckets(): Promise<VikunjaBucket[]> {
    return this.api<VikunjaBucket[]>(
      'GET',
      `/projects/${this.board.projectId}/views/${this.board.viewId}/tasks`
    );
  }

  async fetchTask(apiId: number): Promise<VikunjaTask> {
    return this.api<VikunjaTask>('GET', `/tasks/${apiId}`);
  }

  /** Fetch all tasks via paginated project endpoint (no per-bucket cap) */
  async fetchAllTasks(): Promise<VikunjaTask[]> {
    const all: VikunjaTask[] = [];
    for (let page = 1; page < 30; page++) {
      const tasks = await this.api<VikunjaTask[]>(
        'GET',
        `/projects/${this.board.projectId}/tasks?per_page=50&page=${page}`
      );
      if (!tasks || tasks.length === 0) break;
      all.push(...tasks);
    }
    return all;
  }

  /** List all tasks, parsed with metadata */
  async list(): Promise<BoardTask[]> {
    const buckets = await this.fetchBuckets();
    const tasks: BoardTask[] = [];

    for (const bucket of buckets) {
      for (const task of bucket.tasks || []) {
        tasks.push(this.parseTask(task, bucket.title));
      }
    }
    return tasks;
  }

  /** List tasks grouped by status */
  async listGrouped(): Promise<Map<string, BoardTask[]>> {
    const buckets = await this.fetchBuckets();
    const grouped = new Map<string, BoardTask[]>();

    for (const bucket of buckets) {
      const tasks = (bucket.tasks || []).map(t => this.parseTask(t, bucket.title));
      if (tasks.length > 0) {
        grouped.set(bucket.title, tasks);
      }
    }
    return grouped;
  }

  /** List tasks for a specific role */
  async mine(role: string): Promise<BoardTask[]> {
    const all = await this.list();
    return all.filter(t => t.owner.toLowerCase() === role.toLowerCase());
  }

  /** Get a single task by display index */
  async view(index: number): Promise<BoardTask> {
    const apiId = await this.resolveIndex(index);
    const task = await this.fetchTask(apiId);
    const buckets = await this.fetchBuckets();
    const status = this.findTaskBucket(task.id, buckets);
    return this.parseTask(task, status);
  }

  /** Create a new task */
  async add(title: string, opts?: {
    status?: string;
    owner?: string;
    priority?: string;
    domain?: string;
    description?: string;
    product?: string;
    chunk?: string;
    sequence?: string;
    type?: string;
  }): Promise<BoardTask> {
    const result = await this.api<VikunjaTask>(
      'PUT',
      `/projects/${this.board.projectId}/tasks`,
      { title, description: opts?.description || '' }
    );

    const bucketId = resolveBucket(this.board, opts?.status || 'later');
    await this.moveToBucket(result.id, bucketId);

    if (opts?.owner) {
      const labelId = LABELS.owner[opts.owner.toLowerCase()];
      if (labelId) await this.addLabel(result.id, labelId);
    }
    if (opts?.priority) {
      const labelId = LABELS.priority[opts.priority.toUpperCase()];
      if (labelId) await this.addLabel(result.id, labelId);
    }
    if (opts?.domain) {
      const labelId = LABELS.domain[opts.domain.toLowerCase()];
      if (labelId) await this.addLabel(result.id, labelId);
    }
    if (opts?.product) {
      const labelId = LABELS.product[opts.product.toLowerCase()];
      if (labelId) await this.addLabel(result.id, labelId);
    }
    if (opts?.chunk) {
      const labelId = LABELS.chunk[opts.chunk.toLowerCase()];
      if (labelId) await this.addLabel(result.id, labelId);
    }
    if (opts?.sequence) {
      const labelId = LABELS.sequence[opts.sequence.toLowerCase()];
      if (labelId) await this.addLabel(result.id, labelId);
    }
    if (opts?.type) {
      const labelId = LABELS.type[opts.type.toLowerCase()];
      if (labelId) await this.addLabel(result.id, labelId);
    }

    this.clearCache();
    const statusName = this.board.bucketNames[bucketId] || opts?.status || 'Later';
    return this.parseTask({ ...result, labels: [] }, statusName);
  }

  /** Move a task to a new status */
  async move(index: number, status: string): Promise<void> {
    const apiId = await this.resolveIndex(index);
    const bucketId = resolveBucket(this.board, status);
    await this.moveToBucket(apiId, bucketId);
    this.clearCache();
  }

  /** Mark a task as done */
  async done(index: number): Promise<void> {
    const apiId = await this.resolveIndex(index);
    const bucketId = this.board.buckets['done'];
    await this.moveToBucket(apiId, bucketId);
    this.clearCache();
  }

  /** Block a task with a reason */
  async block(index: number, reason?: string): Promise<void> {
    const apiId = await this.resolveIndex(index);
    const bucketId = this.board.buckets['blocked'];
    await this.moveToBucket(apiId, bucketId);
    if (reason) {
      await this.addComment(apiId, `BLOCKED: ${reason}`);
    }
    this.clearCache();
  }

  /** Unblock a task — move back to Next */
  async unblock(index: number): Promise<void> {
    const apiId = await this.resolveIndex(index);
    const bucketId = this.board.buckets['next'];
    await this.moveToBucket(apiId, bucketId);
    await this.addComment(apiId, 'Unblocked — moved to Next');
    this.clearCache();
  }

  /** Update a task's title and/or description */
  async update(index: number, fields: { title?: string; description?: string; product?: string }): Promise<void> {
    const apiId = await this.resolveIndex(index);
    const hasTitle = fields.title !== undefined;
    const hasDesc = fields.description !== undefined;
    if (hasTitle || hasDesc) {
      // Vikunja POST zeros omitted fields — read current, merge, send full payload
      const current = await this.fetchTask(apiId);
      const payload = {
        title: hasTitle ? fields.title : current.title,
        description: hasDesc ? fields.description : (current.description || ''),
      };
      await this.api('POST', `/tasks/${apiId}`, payload);
    }
    if (fields.product) {
      const labelId = LABELS.product[fields.product.toLowerCase()];
      if (labelId) await this.addLabel(apiId, labelId);
    }
    this.clearCache();
  }

  /** Add a label to an existing task by index */
  async tag(index: number, category: string, value: string): Promise<void> {
    const apiId = await this.resolveIndex(index);
    const group = (LABELS as Record<string, Record<string, number>>)[category];
    if (!group) throw new Error(`Unknown label category "${category}". Valid: ${Object.keys(LABELS).join(', ')}`);
    const labelId = group[value.toLowerCase()] || group[value.toUpperCase()] || group[value];
    if (!labelId) throw new Error(`Unknown ${category} "${value}". Valid: ${Object.keys(group).join(', ')}`);

    // Remove existing labels in the same category before adding new one
    // This ensures domain/chunk/sequence tags replace, not append
    const categoryLabelIds = new Set(Object.values(group));
    const task = await this.fetchTask(apiId);
    for (const label of task.labels || []) {
      if (categoryLabelIds.has(label.id) && label.id !== labelId) {
        await this.removeLabel(apiId, label.id);
      }
    }

    try {
      await this.addLabel(apiId, labelId);
    } catch (err: any) {
      // Ignore duplicate label errors (already tagged)
      if (err?.message?.includes('409') || err?.message?.includes('already')) return;
      throw err;
    }
  }

  /** Remove a label by category and value */
  async untag(index: number, category: string, value: string): Promise<void> {
    const apiId = await this.resolveIndex(index);
    const group = (LABELS as Record<string, Record<string, number>>)[category];
    if (!group) throw new Error(`Unknown label category "${category}". Valid: ${Object.keys(LABELS).join(', ')}`);
    const labelId = group[value.toLowerCase()];
    if (!labelId) throw new Error(`Unknown ${category} "${value}". Valid: ${Object.keys(group).join(', ')}`);
    await this.removeLabel(apiId, labelId);
  }

  /** Add a comment to a task */
  async comment(index: number, text: string): Promise<void> {
    const apiId = await this.resolveIndex(index);
    await this.addComment(apiId, text);
  }

  /** Get comments for a task */
  async comments(index: number): Promise<Array<{ author: string; text: string }>> {
    const apiId = await this.resolveIndex(index);
    const raw = await this.api<Array<{ author?: { username?: string }; comment: string }>>(
      'GET', `/tasks/${apiId}/comments`
    );
    return (raw || []).map(c => ({
      author: c.author?.username || 'unknown',
      text: c.comment,
    }));
  }

  // ── Snapshot (for audit gate) ──

  /** Capture current board state as a snapshot */
  async snapshot(): Promise<{ board: string; timestamp: string; tasks: BoardTask[] }> {
    const tasks = await this.list();
    return {
      board: this.board.name,
      timestamp: new Date().toISOString(),
      tasks,
    };
  }

  // ── Bucket admin ──

  /** List all buckets with their WIP limits and task counts */
  async fetchBucketsWithLimits(): Promise<Array<{ id: number; title: string; limit: number; taskCount: number }>> {
    const buckets = await this.fetchBuckets();
    return buckets.map(b => ({
      id: b.id,
      title: b.title,
      limit: b.limit ?? 0,
      taskCount: (b.tasks || []).length,
    }));
  }

  /** Set the WIP limit for a bucket */
  async createLabel(title: string): Promise<{ id: number; title: string }> {
    return this.api<{ id: number; title: string }>('PUT', '/labels', { title });
  }

  async listLabels(): Promise<Array<{ id: number; title: string }>> {
    // Vikunja paginates at 50 — fetch all pages
    const all: Array<{ id: number; title: string }> = [];
    let page = 1;
    while (true) {
      const batch = await this.api<Array<{ id: number; title: string }>>('GET', `/labels?page=${page}`);
      all.push(...batch);
      if (batch.length < 50) break;
      page++;
    }
    return all;
  }

  async deleteLabel(id: number): Promise<void> {
    await this.api<void>('DELETE', `/labels/${id}`);
  }

  async setBucketLimit(bucketId: number, limit: number): Promise<void> {
    const buckets = await this.fetchBuckets();
    const bucket = buckets.find(b => b.id === bucketId);
    const title = bucket?.title || 'Unknown';
    await this.api(
      'POST',
      `/projects/${this.board.projectId}/views/${this.board.viewId}/buckets/${bucketId}`,
      { title, limit }
    );
  }

  // ── Now awareness ──

  /** Get cards in Now for a specific role */
  async now(role: string): Promise<BoardTask[]> {
    const all = await this.list();
    return all.filter(
      t => t.status === 'Now' && t.owner.toLowerCase() === role.toLowerCase()
    );
  }

  // ── Internals ──

  private async moveToBucket(apiId: number, bucketId: number): Promise<void> {
    // Read current task state — needed for done-flag sync (#1315)
    let savedTask: VikunjaTask | null = null;
    try {
      savedTask = await this.fetchTask(apiId);
    } catch { /* best effort */ }

    try {
      await this.api(
        'POST',
        `/projects/${this.board.projectId}/views/${this.board.viewId}/buckets/${bucketId}/tasks`,
        { task_id: apiId }
      );
    } catch (err: any) {
      const msg = err?.message || String(err);
      if (msg.includes('412') || msg.includes('bucket limit')) {
        const bucketName = this.board.bucketNames[bucketId] || 'target column';
        // Fetch the current limit for a better message
        let limitInfo = '';
        try {
          const buckets = await this.fetchBucketsWithLimits();
          const target = buckets.find(b => b.id === bucketId);
          if (target) {
            limitInfo = ` (limit: ${target.limit}, current: ${target.taskCount})`;
          }
        } catch { /* best effort */ }
        throw new Error(
          `${bucketName} column is full${limitInfo}. Move something to Done first.`
        );
      }
      throw err;
    }
    // Sync done flag — must include title+description or Vikunja zeros them (#1315)
    const isDone = bucketId === this.board.buckets['done'];
    const current = savedTask || await this.fetchTask(apiId);
    await this.api('POST', `/tasks/${apiId}`, {
      title: current.title,
      description: current.description || '',
      done: isDone,
    });
  }

  private async addLabel(apiId: number, labelId: number): Promise<void> {
    await this.api('PUT', `/tasks/${apiId}/labels`, { label_id: labelId });
  }

  async removeLabel(apiId: number, labelId: number): Promise<void> {
    await this.api('DELETE', `/tasks/${apiId}/labels/${labelId}`, undefined);
  }

  /** Add a task relation (sequencing) (#1636) */
  async addRelation(index: number, otherIndex: number, kind: string = 'blocked'): Promise<void> {
    const apiId = await this.resolveIndex(index);
    const otherApiId = await this.resolveIndex(otherIndex);
    await this.api('PUT', `/tasks/${apiId}/relations`, { other_task_id: otherApiId, relation_kind: kind });
  }

  /** Remove a task relation */
  async removeRelation(index: number, otherIndex: number, kind: string = 'blocked'): Promise<void> {
    const apiId = await this.resolveIndex(index);
    const otherApiId = await this.resolveIndex(otherIndex);
    await this.api('DELETE', `/tasks/${apiId}/relations/${kind}/${otherApiId}`, undefined);
  }

  /** Get task relations (returns display indices, not API IDs) */
  async getRelations(index: number): Promise<{ blockedBy: number[]; blocks: number[] }> {
    const apiId = await this.resolveIndex(index);
    const task = await this.fetchTask(apiId);
    const related = (task as any).related_tasks || {};

    // Build reverse map: API ID → display index
    const map = await this.buildTaskMap();
    const reverseMap = new Map<number, number>();
    for (const [displayIdx, apiIdx] of map) {
      reverseMap.set(apiIdx, displayIdx);
    }

    const blockedBy = (related['blocked'] || []).map((t: any) => reverseMap.get(t.id) || t.id);
    const blocks = (related['blocking'] || []).map((t: any) => reverseMap.get(t.id) || t.id);

    return { blockedBy, blocks };
  }

  /** Swap the owner label on a task */
  async reassignOwner(index: number, newOwner: string): Promise<{ oldOwner: string; newOwner: string }> {
    const apiId = await this.resolveIndex(index);
    const task = await this.fetchTask(apiId);

    // Find and remove existing owner label
    let oldOwner = '';
    for (const label of task.labels || []) {
      if (label.title.startsWith('owner:')) {
        oldOwner = label.title.split(':')[1];
        await this.removeLabel(apiId, label.id);
        break;
      }
    }

    // Add new owner label
    const newLabelId = LABELS.owner[newOwner.toLowerCase()];
    if (!newLabelId) {
      throw new Error(`Unknown owner "${newOwner}". Valid: ${Object.keys(LABELS.owner).join(', ')}`);
    }
    await this.addLabel(apiId, newLabelId);
    this.clearCache();

    return { oldOwner, newOwner: newOwner.charAt(0).toUpperCase() + newOwner.slice(1) };
  }

  private async addComment(apiId: number, comment: string): Promise<void> {
    await this.api('PUT', `/tasks/${apiId}/comments`, { comment });
  }

  private findTaskBucket(apiId: number, buckets: VikunjaBucket[]): string {
    for (const bucket of buckets) {
      for (const task of bucket.tasks || []) {
        if (task.id === apiId) return bucket.title;
      }
    }
    return 'Unknown';
  }

  private parseTask(task: VikunjaTask, status: string): BoardTask {
    let owner = '';
    let priority = '';
    const domains: string[] = [];

    for (const label of task.labels || []) {
      if (label.title.startsWith('owner:')) {
        owner = label.title.split(':')[1];
      } else if (['P1', 'P2', 'P3'].includes(label.title)) {
        priority = label.title;
      } else {
        domains.push(label.title);
      }
    }

    return {
      index: task.index ?? task.id,
      apiId: task.id,
      title: task.title,
      description: task.description || '',
      status,
      owner: owner.charAt(0).toUpperCase() + owner.slice(1),
      priority,
      domains,
      done: task.done,
      created: task.created,
      updated: task.updated,
    };
  }
}
