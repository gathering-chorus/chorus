/**
 * sdk.ts lifecycle tests (#2241 wave 2 pt 2).
 *
 * Covers the add / move / done / update / comment / reassign / set /
 * tag lifecycle via mock BoardClient. These are the sdk functions roles
 * actually invoke through the CLI; covering them is the real lift on
 * products/cards coverage.
 */

import type { BoardClient } from '../src/client';
import type { BoardTask } from '../src/types';
import {
  addCard, moveCard, doneCard, updateCard, commentCard,
  reassignCard, setCard, tagCard, untagCard,
} from '../src/sdk';

class MockClient {
  boardName = 'gathering';
  calls: Array<{ method: string; args: unknown[] }> = [];
  tasks: Map<number, BoardTask> = new Map();

  private record(method: string, args: unknown[]) {
    this.calls.push({ method, args });
  }

  async add(title: string, opts?: {
    status?: string; owner?: string; priority?: string; domain?: string; description?: string;
    product?: string;
  }): Promise<BoardTask> {
    this.record('add', [title, opts]);
    const index = (this.tasks.size + 1) * 100;
    const task: BoardTask = {
      index, title, description: opts?.description ?? '',
      status: opts?.status ?? 'Later',
      owner: opts?.owner ?? 'Kade',
      priority: opts?.priority ?? 'P2',
      domains: [], product: opts?.product,
      apiId: index + 1, done: false,
      created: '2026-04-19T10:00:00Z', updated: '2026-04-19T10:00:00Z',
    } as unknown as BoardTask;
    this.tasks.set(index, task);
    return task;
  }

  async view(index: number): Promise<BoardTask> {
    this.record('view', [index]);
    const t = this.tasks.get(index);
    if (!t) throw new Error(`not found ${index}`);
    return t;
  }

  async move(index: number, status: string): Promise<void> {
    this.record('move', [index, status]);
    const t = this.tasks.get(index);
    if (t) (t as { status: string }).status = status;
  }

  async done(index: number): Promise<void> {
    this.record('done', [index]);
    // #2707 — model done() effect: flip task status so doneCard's
    // verify-after-move sees Done. Without this, mock looks like silent-fail.
    const t = this.tasks.get(index);
    if (t) (t as { status: string }).status = 'Done';
  }

  async update(index: number, fields: unknown): Promise<void> {
    this.record('update', [index, fields]);
  }

  async comment(index: number, text: string): Promise<void> {
    this.record('comment', [index, text]);
  }

  async comments(_index: number): Promise<Array<{ author: string; text: string }>> {
    this.record('comments', [_index]);
    return [];
  }

  async tag(index: number, category: string, value: string): Promise<void> {
    this.record('tag', [index, category, value]);
  }

  async untag(index: number, category: string, value: string): Promise<void> {
    this.record('untag', [index, category, value]);
  }

  // #3267: chunk (and subproduct/subdomain) route through this auto-create path.
  async applyLabelByName(index: number, labelName: string): Promise<{ labelId: number; created: boolean }> {
    this.record('applyLabelByName', [index, labelName]);
    return { labelId: 999, created: true };
  }

  async reassignOwner(index: number, role: string): Promise<{ oldOwner: string; newOwner: string }> {
    this.record('reassignOwner', [index, role]);
    const t = this.tasks.get(index);
    const oldOwner = (t as { owner?: string } | undefined)?.owner ?? '';
    if (t) (t as { owner?: string }).owner = role;
    return { oldOwner, newOwner: role };
  }

  async setField(index: number, field: string, value: string): Promise<void> {
    this.record('setField', [index, field, value]);
  }
}

function asBoardClient(m: MockClient): BoardClient {
  return m as unknown as BoardClient;
}

function silenceConsole() {
  const origLog = console.log;
  const origErr = console.error;
  const logs: string[] = [];
  const errs: string[] = [];
  console.log = (...a) => logs.push(a.join(' '));
  console.error = (...a) => errs.push(a.join(' '));
  return {
    logs, errs,
    restore: () => { console.log = origLog; console.error = origErr; },
  };
}

function interceptExit(): { calls: number[]; restore: () => void } {
  const calls: number[] = [];
  const orig = process.exit;
  process.exit = ((code?: number) => {
    calls.push(code ?? 0);
    throw new Error(`process.exit(${code})`);
  }) as typeof process.exit;
  return {
    calls,
    restore: () => { process.exit = orig; },
  };
}

describe('addCard — validation', () => {
  it('fails with accumulated errors when required fields missing', async () => {
    const mock = new MockClient();
    const cap = silenceConsole();
    const exit = interceptExit();
    try {
      await addCard(asBoardClient(mock), 'build something new', {}).catch(() => {});
    } finally {
      exit.restore();
      cap.restore();
    }
    expect(exit.calls).toEqual([1]);
    // Errors mention domain, priority. type inferred from "build" verb, origin inferred from type.
    const errBlob = cap.errs.join('\n');
    expect(errBlob).toMatch(/--domain/);
    expect(errBlob).toMatch(/--priority/);
  });

  it('quick mode exempts description requirement', async () => {
    const mock = new MockClient();
    const cap = silenceConsole();
    try {
      const t = await addCard(asBoardClient(mock), 'fix the thing', {
        domain: 'chorus', priority: 'P2', quick: true,
      });
      expect(t.title).toBe('fix the thing');
    } finally {
      cap.restore();
    }
    const addCall = mock.calls.find((c) => c.method === 'add');
    expect(addCall).toBeDefined();
  });

  it('auto-tags type from title verb (fix → type:fix)', async () => {
    const mock = new MockClient();
    const cap = silenceConsole();
    try {
      await addCard(asBoardClient(mock), 'fix stale timestamps', {
        domain: 'chorus', priority: 'P1', quick: true,
      });
    } finally {
      cap.restore();
    }
    expect(cap.logs.some((l) => l.includes('type:fix'))).toBe(true);
  });

  it('auto-tags origin from type (fix → reactive)', async () => {
    const mock = new MockClient();
    const cap = silenceConsole();
    try {
      await addCard(asBoardClient(mock), 'fix stale timestamps', {
        domain: 'chorus', priority: 'P1', quick: true,
      });
    } finally {
      cap.restore();
    }
    expect(cap.logs.some((l) => l.includes('origin:reactive'))).toBe(true);
  });

  it('passes sequence + origin tag calls after add', async () => {
    const mock = new MockClient();
    const cap = silenceConsole();
    try {
      await addCard(asBoardClient(mock), 'fix flaky tests', {
        domain: 'chorus', priority: 'P1', quick: true, sequence: 'quality',
      });
    } finally {
      cap.restore();
    }
    const tagCalls = mock.calls.filter((c) => c.method === 'tag');
    expect(tagCalls.some((c) => c.args[1] === 'sequence' && c.args[2] === 'quality')).toBe(true);
    expect(tagCalls.some((c) => c.args[1] === 'origin')).toBe(true);
  });

  it('unknown origin value → error', async () => {
    const mock = new MockClient();
    const cap = silenceConsole();
    const exit = interceptExit();
    try {
      await addCard(asBoardClient(mock), 'photograph something', {
        domain: 'chorus', priority: 'P1', type: 'new', origin: 'whimsical', quick: true,
      }).catch(() => {});
    } finally {
      exit.restore();
      cap.restore();
    }
    expect(exit.calls).toEqual([1]);
    expect(cap.errs.join('\n')).toMatch(/Unknown origin "whimsical"/);
  });

  it('non-quick with description + AC checkbox passes without error', async () => {
    const mock = new MockClient();
    const cap = silenceConsole();
    try {
      await addCard(asBoardClient(mock), 'fix thing', {
        domain: 'chorus', priority: 'P1', sequence: 'chorus',
        description: '## Experience\nJeff sees\n## Why this matters\nWithout this fix the X path silently drops events that downstream consumers depend on, and the Y page renders stale data until restart. This affects normal usage weekly for multiple roles plus Jeff during active sessions, not an edge case.\n## Why it helps Chorus\nThe spine becomes signal-rich for all three roles plus Jeff. Direct improvement to chorus_logs_for_card joins and recent-errors queries, the surface the whole team reads when something breaks during a card.\n## Why it\'s not gold plating or a nit\nThe noise is currently masking real failure modes; I missed two yesterday because tunnel-probe events drowned the signal. Cosmetic would be reformatting; this is recovering load-bearing observability for the team.\n## Dependencies\nNone external. One file in chorus-hooks observer.rs; ships with the same build pipeline as the rest of the hook crate.\n## Scope of impact\nEvery observer.error emit and every chorus_logs_for_card query touch this. All three agent roles plus Jeff read the resulting spine surface. No API shape change.\n## AC\n- [ ] first\n- [ ] second',
      });
    } finally { cap.restore(); }
    expect(mock.calls.find((c) => c.method === 'add')).toBeDefined();
  });

  it('non-quick with numbered-list AC also passes', async () => {
    const mock = new MockClient();
    const cap = silenceConsole();
    try {
      await addCard(asBoardClient(mock), 'fix thing', {
        domain: 'chorus', priority: 'P1', sequence: 'chorus',
        description: '## Experience\nok\n## Why this matters\nWithout this fix the X path silently drops events that downstream consumers depend on, and the Y page renders stale data until restart. This affects normal usage weekly for multiple roles plus Jeff during active sessions, not an edge case.\n## Why it helps Chorus\nThe spine becomes signal-rich for all three roles plus Jeff. Direct improvement to chorus_logs_for_card joins and recent-errors queries, the surface the whole team reads when something breaks during a card.\n## Why it\'s not gold plating or a nit\nThe noise is currently masking real failure modes; I missed two yesterday because tunnel-probe events drowned the signal. Cosmetic would be reformatting; this is recovering load-bearing observability for the team.\n## Dependencies\nNone external. One file in chorus-hooks observer.rs; ships with the same build pipeline as the rest of the hook crate.\n## Scope of impact\nEvery observer.error emit and every chorus_logs_for_card query touch this. All three agent roles plus Jeff read the resulting spine surface. No API shape change.\n## AC\n1. first\n2. second',
      });
    } finally { cap.restore(); }
    expect(mock.calls.find((c) => c.method === 'add')).toBeDefined();
  });

  it('non-quick without AC in description → error', async () => {
    const mock = new MockClient();
    const cap = silenceConsole();
    const exit = interceptExit();
    try {
      await addCard(asBoardClient(mock), 'fix thing', {
        domain: 'chorus', priority: 'P1',
        description: 'just prose, no AC markers',
      }).catch(() => {});
    } finally {
      exit.restore();
      cap.restore();
    }
    expect(exit.calls).toEqual([1]);
    expect(cap.errs.join('\n')).toMatch(/missing acceptance criteria/);
  });

  it('unknown type value → error', async () => {
    const mock = new MockClient();
    const cap = silenceConsole();
    const exit = interceptExit();
    try {
      await addCard(asBoardClient(mock), 'verb make up', {
        domain: 'chorus', priority: 'P1', type: 'bogus',
      }).catch(() => {});
    } finally {
      exit.restore();
      cap.restore();
    }
    expect(exit.calls).toEqual([1]);
    expect(cap.errs.join('\n')).toMatch(/Unknown type "bogus"/);
  });
});

describe('moveCard', () => {
  it('non-WIP moves pass through without gate enforcement', async () => {
    const mock = new MockClient();
    await mock.add('any', { status: 'Later', description: '## Experience\ntext\n## AC\n- [ ] item' });
    const [index] = Array.from(mock.tasks.keys());
    const cap = silenceConsole();
    try {
      await moveCard(asBoardClient(mock), index, 'Next');
    } finally {
      cap.restore();
    }
    expect(mock.calls.find((c) => c.method === 'move')?.args).toEqual([index, 'Next']);
  });

  it('WIP move with complete description passes gates and moves (non-code card)', async () => {
    const mock = new MockClient();
    // Title avoids CODE_INDICATORS words so blast-radius gate doesn't fire.
    await mock.add('photograph library shelves', {
      description: '## Experience\nJeff sees X\n## AC\n- [ ] y',
    });
    const [index] = Array.from(mock.tasks.keys());
    const cap = silenceConsole();
    try {
      await moveCard(asBoardClient(mock), index, 'WIP');
    } finally {
      cap.restore();
    }
    expect(mock.calls.find((c) => c.method === 'move')).toBeDefined();
  });

  it('WIP move without AC → AC gate blocks (process.exit(1))', async () => {
    const mock = new MockClient();
    // Non-code title so we isolate the AC-gate signal, not blast-radius.
    await mock.add('photograph library', { description: '## Experience\nonly' });
    const [index] = Array.from(mock.tasks.keys());
    const cap = silenceConsole();
    const exit = interceptExit();
    try {
      await moveCard(asBoardClient(mock), index, 'WIP').catch(() => {});
    } finally {
      exit.restore();
      cap.restore();
    }
    expect(exit.calls).toEqual([1]);
  });

  it('swat card can move to WIP regardless of description', async () => {
    const mock = new MockClient();
    // [swat] bypasses AC/Experience/taxonomy gates AND isCodeCard classification
    // typically returns false via [fix]/[swat] brackets, but we also use a
    // non-code base title to guarantee no blast-radius call.
    await mock.add('[swat] photograph now', { description: '' });
    const [index] = Array.from(mock.tasks.keys());
    const cap = silenceConsole();
    try {
      await moveCard(asBoardClient(mock), index, 'WIP');
    } finally {
      cap.restore();
    }
    expect(mock.calls.find((c) => c.method === 'move')).toBeDefined();
  });
});

describe('doneCard', () => {
  it('invokes client.done with index', async () => {
    const mock = new MockClient();
    await mock.add('something', { status: 'WIP', description: '## Experience\nx\n## AC\n- [ ] y' });
    const [index] = Array.from(mock.tasks.keys());
    const cap = silenceConsole();
    try {
      await doneCard(asBoardClient(mock), index, ['proven-id-1']);
    } finally {
      cap.restore();
    }
    const doneCall = mock.calls.find((c) => c.method === 'done');
    expect(doneCall?.args[0]).toBe(index);
  });
});

describe('updateCard', () => {
  it('calls client.update with the provided fields', async () => {
    const mock = new MockClient();
    await mock.add('old', {});
    const [index] = Array.from(mock.tasks.keys());
    const cap = silenceConsole();
    try {
      await updateCard(asBoardClient(mock), index, { title: 'new title' });
    } finally {
      cap.restore();
    }
    const upd = mock.calls.find((c) => c.method === 'update');
    expect(upd?.args[0]).toBe(index);
    expect(upd?.args[1]).toMatchObject({ title: 'new title' });
  });
});

describe('commentCard', () => {
  it('calls client.comment with index + text', async () => {
    const mock = new MockClient();
    await mock.add('card', {});
    const [index] = Array.from(mock.tasks.keys());
    const cap = silenceConsole();
    try {
      await commentCard(asBoardClient(mock), index, 'hello from test');
    } finally {
      cap.restore();
    }
    const c = mock.calls.find((c) => c.method === 'comment');
    expect(c?.args).toEqual([index, 'hello from test']);
  });
});

describe('reassignCard', () => {
  it('updates owner via tag calls (remove old, add new)', async () => {
    const mock = new MockClient();
    await mock.add('card', { owner: 'Kade' });
    const [index] = Array.from(mock.tasks.keys());
    const cap = silenceConsole();
    try {
      await reassignCard(asBoardClient(mock), index, 'wren');
    } finally {
      cap.restore();
    }
    // Implementation calls client.reassignOwner — a single named method that
    // swaps the owner:* label server-side.
    const r = mock.calls.find((c) => c.method === 'reassignOwner');
    expect(r).toBeDefined();
    expect(r!.args).toEqual([index, 'wren']);
  });

  it('rejects invalid role with process.exit(1)', async () => {
    const mock = new MockClient();
    await mock.add('card', {});
    const [index] = Array.from(mock.tasks.keys());
    const cap = silenceConsole();
    const exit = interceptExit();
    try {
      await reassignCard(asBoardClient(mock), index, 'nobody').catch(() => {});
    } finally {
      exit.restore();
      cap.restore();
    }
    expect(exit.calls).toEqual([1]);
    expect(cap.errs.join('\n')).toMatch(/Invalid role "nobody"/);
  });
});

describe('setCard', () => {
  it('applies a pairs object to the card via tag calls', async () => {
    const mock = new MockClient();
    await mock.add('card', {});
    const [index] = Array.from(mock.tasks.keys());
    const cap = silenceConsole();
    try {
      await setCard(asBoardClient(mock), index, { sequence: 'quality' });
    } finally {
      cap.restore();
    }
    const hasTag = mock.calls.some((c) => c.method === 'tag');
    expect(hasTag).toBe(true);
  });

  // #3267: chunk is now a dynamic priority axis — it must route through
  // applyLabelByName (auto-create the label by name), NOT the static-map tag
  // path that rejects any chunk not pre-registered in config.ts.
  it('routes chunk through applyLabelByName (auto-create), not the static-map tag', async () => {
    const mock = new MockClient();
    await mock.add('card', {});
    const [index] = Array.from(mock.tasks.keys());
    const cap = silenceConsole();
    try {
      await setCard(asBoardClient(mock), index, { chunk: 'werk' });
    } finally {
      cap.restore();
    }
    const viaApply = mock.calls.some((c) => c.method === 'applyLabelByName' && (c.args[1] as string) === 'chunk:werk');
    const viaStaticTag = mock.calls.some((c) => c.method === 'tag' && (c.args[1] as string) === 'chunk');
    expect(viaApply).toBe(true);      // chunk auto-creates its label by name
    expect(viaStaticTag).toBe(false); // not the static-map path that rejects unknowns
  });
});

describe('tagCard / untagCard', () => {
  it('tagCard passes category + value through', async () => {
    const mock = new MockClient();
    await mock.add('card', {});
    const [index] = Array.from(mock.tasks.keys());
    const cap = silenceConsole();
    try {
      await tagCard(asBoardClient(mock), index, 'coordination', 'sequence');
    } finally {
      cap.restore();
    }
    const t = mock.calls.find((c) => c.method === 'tag');
    expect(t).toBeDefined();
  });

  it('untagCard passes category + value through', async () => {
    const mock = new MockClient();
    await mock.add('card', {});
    const [index] = Array.from(mock.tasks.keys());
    // #2652 AC7 — idempotency: untag is now a no-op if the label isn't present.
    // Seed the label so the test exercises the pass-through path it intends.
    const seeded = mock.tasks.get(index);
    if (seeded) (seeded as { domains: string[] }).domains = ['sequence:coordination'];
    const cap = silenceConsole();
    try {
      await untagCard(asBoardClient(mock), index, 'coordination', 'sequence');
    } finally {
      cap.restore();
    }
    const t = mock.calls.find((c) => c.method === 'untag');
    expect(t).toBeDefined();
  });
});
