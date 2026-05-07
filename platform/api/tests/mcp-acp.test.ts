/**
 * #2750 slice 2 — MCP chorus_acp atomic transaction.
 *
 * /acp loop today is 7 steps the model executes (pull → commit → push → PR
 * open → PR merge → cards done → spine + branch close). Skills are markdown
 * — model compliance is unreliable; today's session has live receipts of
 * shortcut + improvised steps. This card moves execution to substrate:
 * one MCP call runs the whole transaction deterministically. The /acp
 * skill collapses to "call this one thing."
 *
 * Per DEC-1674 (TDD): RED first, then handler. The specs describe
 * Jeff's experience: he says /acp 2750 once and the work goes from werk
 * to merged-on-main with the card Done — no manual gh-merge, no manual
 * cleanup, no remembering which step is next.
 */
import { buildMcpServer } from '../src/mcp/server';

const oneCard = [{ id: 2750, owner: 'kade', title: 'chorus_acp atomic' }];

describe('#2750 slice 2 — chorus_acp MCP atomic transaction', () => {
  describe('AC1 — registration', () => {
    test('exposes chorus_acp in tools/list', async () => {
      const server = buildMcpServer(() => 'kade');
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/list');
      const result = await handler({ method: 'tools/list', params: {} }, {});
      const names = result.tools.map((t: { name: string }) => t.name);
      expect(names).toContain('chorus_acp');
    });

    test('input schema accepts only role — strict, card_id derived from board', async () => {
      const server = buildMcpServer(() => 'kade');
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/list');
      const result = await handler({ method: 'tools/list', params: {} }, {});
      const tool = result.tools.find((t: { name: string }) => t.name === 'chorus_acp');
      expect(tool).toBeDefined();
      expect(tool.inputSchema.required).toEqual(['role']);
      expect(tool.inputSchema.additionalProperties).toBe(false);
      expect(tool.inputSchema.properties.role.enum.sort()).toEqual(['kade', 'silas', 'wren']);
    });
  });

  describe('AC2 — atomic happy path', () => {
    function buildHappyExec() {
      const calls: Array<{ file: string; args: string[]; cwd?: string }> = [];
      const fn = jest.fn(async (file: string, args: string[], opts: { cwd?: string } = {}) => {
        calls.push({ file, args, cwd: opts.cwd });
        if (file.endsWith('git-queue.sh') && args[0] === 'commit') {
          return { stdout: '[kade/2750 abcd5678] kade: m\n', stderr: '' };
        }
        if (file.endsWith('git-queue.sh') && args[0] === 'push') return { stdout: 'pushed\n', stderr: '' };
        if (file === 'git' && args[0] === 'rev-parse') return { stdout: 'kade/2750\n', stderr: '' };
        if (file === 'gh' && args[0] === 'pr' && args[1] === 'view') {
          // Pretend PR doesn't exist yet (first call) so handler creates it
          const err = new Error('no PR');
          (err as { code?: number }).code = 1;
          throw err;
        }
        if (file === 'gh' && args[0] === 'pr' && args[1] === 'create') {
          return { stdout: 'https://github.com/x/y/pull/999\n', stderr: '' };
        }
        if (file === 'gh' && args[0] === 'pr' && args[1] === 'merge') {
          return { stdout: 'merged\n', stderr: '' };
        }
        if (file.endsWith('cards') && args[0] === 'done') {
          return { stdout: 'Done: #2750\n', stderr: '' };
        }
        if (file.endsWith('chorus-log')) return { stdout: '', stderr: '' };
        if (file.endsWith('chorus-werk') && args[0] === 'close') return { stdout: 'closed\n', stderr: '' };
        throw new Error(`unexpected: ${file} ${args.join(' ')}`);
      });
      return { fn, calls };
    }

    test('runs pull + commit + push + PR open + PR merge + cards done + spine + werk close in order', async () => {
      const { fn: exec, calls } = buildHappyExec();
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'kade', {
        boardReader: (async () => ({ ok: true, cards: oneCard })) as never,
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/platform/scripts/git-queue.sh',
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      const result = await handler(
        { method: 'tools/call', params: { name: 'chorus_acp', arguments: { role: 'kade' } } },
        {},
      );

      // Order: commit → push → pr view → pr create → pr merge → cards done → werk close
      const ordered = calls.map((c) => `${c.file.split('/').pop()}:${c.args[0]}${c.args[1] ? '/' + c.args[1] : ''}`);
      const has = (sub: string) => ordered.some((line) => line.includes(sub));
      expect(has('git-queue.sh:commit')).toBe(true);
      expect(has('git-queue.sh:push')).toBe(true);
      expect(has('gh:pr/merge')).toBe(true);
      expect(has('cards:done')).toBe(true);
      expect(has('chorus-werk:close')).toBe(true);

      // Spine event emitted on success
      expect(events.find((e) => e.event === 'card.accepted')).toBeDefined();

      // Result shape
      const text = result.content[0].text;
      const parsed = JSON.parse(text);
      expect(parsed.role).toBe('kade');
      expect(parsed.card_id).toBe(2750);
      expect(parsed.sha).toBe('abcd5678');
    });
  });

  describe('AC3 — refusal taxonomy', () => {
    test('refuses hook-fail when commit blocked by pre-commit', async () => {
      const exec = jest.fn(async (file: string, args: string[]) => {
        if (file.endsWith('git-queue.sh') && args[0] === 'commit') {
          const err = new Error('cmd failed');
          (err as { code?: number; stderr?: string }).code = 1;
          (err as { stderr?: string }).stderr = 'pre-commit: 🔴 1/5 checks failed';
          throw err;
        }
        return { stdout: '', stderr: '' };
      });
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'kade', {
        boardReader: (async () => ({ ok: true, cards: oneCard })) as never,
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/platform/scripts/git-queue.sh',
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      await expect(
        handler({ method: 'tools/call', params: { name: 'chorus_acp', arguments: { role: 'kade' } } }, {}),
      ).rejects.toThrow(/hook-fail/);
      expect(events.find((e) => e.event === 'chorus_acp.refused')?.fields.reason).toBe('hook-fail');
    });

    test('refuses pr-merge-fail when gh pr merge fails', async () => {
      const exec = jest.fn(async (file: string, args: string[]) => {
        if (file.endsWith('git-queue.sh') && args[0] === 'commit') return { stdout: '[kade/2750 abcd] kade: m\n', stderr: '' };
        if (file.endsWith('git-queue.sh') && args[0] === 'push') return { stdout: 'pushed\n', stderr: '' };
        if (file === 'git' && args[0] === 'rev-parse') return { stdout: 'kade/2750\n', stderr: '' };
        if (file === 'gh' && args[1] === 'view') { const e = new Error('no PR') as { code?: number }; e.code = 1; throw e; }
        if (file === 'gh' && args[1] === 'create') return { stdout: 'https://x/pr/1\n', stderr: '' };
        if (file === 'gh' && args[1] === 'merge') {
          const err = new Error('merge failed');
          (err as { code?: number; stderr?: string }).code = 1;
          (err as { stderr?: string }).stderr = 'mergeable status: BLOCKED';
          throw err;
        }
        return { stdout: '', stderr: '' };
      });
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'kade', {
        boardReader: (async () => ({ ok: true, cards: oneCard })) as never,
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/platform/scripts/git-queue.sh',
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      await expect(
        handler({ method: 'tools/call', params: { name: 'chorus_acp', arguments: { role: 'kade' } } }, {}),
      ).rejects.toThrow(/pr-merge-fail/);
      expect(events.find((e) => e.event === 'chorus_acp.refused')?.fields.reason).toBe('pr-merge-fail');
    });
  });

  describe('AC4 — idempotent partial-failure recovery', () => {
    test('skips PR create when PR already exists (idempotent re-run)', async () => {
      const calls: Array<{ args: string[] }> = [];
      const exec = jest.fn(async (file: string, args: string[]) => {
        calls.push({ args });
        if (file.endsWith('git-queue.sh') && args[0] === 'commit') return { stdout: '[kade/2750 abcd] kade: m\n', stderr: '' };
        if (file.endsWith('git-queue.sh') && args[0] === 'push') return { stdout: 'pushed\n', stderr: '' };
        if (file === 'git' && args[0] === 'rev-parse') return { stdout: 'kade/2750\n', stderr: '' };
        if (file === 'gh' && args[1] === 'view') {
          // PR already exists — view succeeds
          return { stdout: 'https://github.com/x/y/pull/999\n', stderr: '' };
        }
        if (file === 'gh' && args[1] === 'merge') return { stdout: 'merged\n', stderr: '' };
        if (file.endsWith('cards') && args[0] === 'done') return { stdout: 'Done: #2750\n', stderr: '' };
        return { stdout: '', stderr: '' };
      });
      const server = buildMcpServer(() => 'kade', {
        boardReader: (async () => ({ ok: true, cards: oneCard })) as never,
        emitSpineEvent: (() => {}) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/platform/scripts/git-queue.sh',
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      await handler({ method: 'tools/call', params: { name: 'chorus_acp', arguments: { role: 'kade' } } }, {});

      // PR create must NOT be in the call list (idempotent: detected existing)
      const createCalls = calls.filter((c) => c.args[0] === 'pr' && c.args[1] === 'create');
      expect(createCalls.length).toBe(0);
      // PR merge SHOULD be in the call list (existing PR can still merge)
      const mergeCalls = calls.filter((c) => c.args[0] === 'pr' && c.args[1] === 'merge');
      expect(mergeCalls.length).toBeGreaterThan(0);
    });
  });

  describe('#2782 — fast-path cards-done must not silently skip when board returns 0 WIP', () => {
    // Today's bug (2026-05-07, hit on #2777, #2778, #2779): /acp re-run on a
    // card whose work is already merged returned `{ branch_closed: true,
    // sha: "unknown" }` but the card stayed at "Later" because the previous
    // run had already moved it to Done, so board returns 0 WIP cards on the
    // second invocation, cardId stays null, and cards-done was gated on
    // `cardId !== null` — silently skipping. The card-state diverges from
    // the branch-state and the report says shipped while the board lies.
    //
    // Fix: when board lookup misses (cards.length !== 1) AND HEAD branch
    // matches `<role>/<id>`, derive cardId from the branch name. The fast-
    // path's "PR already merged" check confirms the branch's identity, so
    // the branch-derived cardId is trustworthy for cards-done.

    test('fast-path with empty WIP board derives cardId from branch and runs cards-done', async () => {
      const calls: Array<{ file: string; args: string[] }> = [];
      const exec = jest.fn(async (file: string, args: string[]) => {
        calls.push({ file, args });
        if (file === 'git' && args[0] === 'rev-parse' && args.includes('HEAD')) {
          return { stdout: 'kade/2779\n', stderr: '' };
        }
        if (file === 'git' && args[0] === 'fetch') return { stdout: '', stderr: '' };
        if (file === 'gh' && args[0] === 'pr' && args[1] === 'view') {
          return { stdout: 'MERGED\n', stderr: '' };
        }
        if (file.endsWith('cards') && args[0] === 'done') {
          return { stdout: 'Done: #2779\n', stderr: '' };
        }
        if (file.endsWith('chorus-werk') && args[0] === 'close') {
          return { stdout: 'closed\n', stderr: '' };
        }
        return { stdout: '', stderr: '' };
      });
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'kade', {
        boardReader: (async () => ({ ok: true, cards: [] })) as never,
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/platform/scripts/git-queue.sh',
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      const result = await handler(
        { method: 'tools/call', params: { name: 'chorus_acp', arguments: { role: 'kade' } } },
        {},
      );

      // cards-done MUST run with the branch-derived cardId (2779)
      const doneCalls = calls.filter((c) => c.file.endsWith('cards') && c.args[0] === 'done');
      expect(doneCalls.length).toBe(1);
      expect(doneCalls[0].args[1]).toBe('2779');

      // Result reports the branch-derived card_id, not null
      const parsed = JSON.parse(result.content[0].text);
      expect(parsed.card_id).toBe(2779);
      expect(parsed.fast_path).toBe(true);

      // Spine event card.accepted carries the resolved id, not null
      const accepted = events.find((e) => e.event === 'card.accepted');
      expect(accepted?.fields.card).toBe(2779);
    });

    test('fast-path with multi-WIP board still derives cardId from branch (single source of truth)', async () => {
      const calls: Array<{ file: string; args: string[] }> = [];
      const exec = jest.fn(async (file: string, args: string[]) => {
        calls.push({ file, args });
        if (file === 'git' && args[0] === 'rev-parse' && args.includes('HEAD')) {
          return { stdout: 'kade/2779\n', stderr: '' };
        }
        if (file === 'git' && args[0] === 'fetch') return { stdout: '', stderr: '' };
        if (file === 'gh' && args[0] === 'pr' && args[1] === 'view') {
          return { stdout: 'MERGED\n', stderr: '' };
        }
        if (file.endsWith('cards') && args[0] === 'done') return { stdout: 'Done\n', stderr: '' };
        if (file.endsWith('chorus-werk') && args[0] === 'close') return { stdout: 'closed\n', stderr: '' };
        return { stdout: '', stderr: '' };
      });
      const server = buildMcpServer(() => 'kade', {
        // Multi-WIP — board returns 2 cards. Branch is the disambiguator.
        boardReader: (async () => ({ ok: true, cards: [{ id: 9999, owner: 'kade', title: 'unrelated' }, { id: 8888, owner: 'kade', title: 'also-unrelated' }] })) as never,
        emitSpineEvent: (() => {}) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/platform/scripts/git-queue.sh',
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      const result = await handler({ method: 'tools/call', params: { name: 'chorus_acp', arguments: { role: 'kade' } } }, {});
      const doneCalls = calls.filter((c) => c.file.endsWith('cards') && c.args[0] === 'done');
      expect(doneCalls.length).toBe(1);
      // Branch (kade/2779) is the source of truth, not the multi-WIP cards (9999, 8888).
      expect(doneCalls[0].args[1]).toBe('2779');
      const parsed = JSON.parse(result.content[0].text);
      expect(parsed.card_id).toBe(2779);
    });
  });

  describe('AC5 — werk-aware', () => {
    test('routes git-queue cwd via resolveWorkingTree (slice 1 reuse)', async () => {
      const cwds: string[] = [];
      const exec = jest.fn(async (file: string, args: string[], opts: { cwd?: string } = {}) => {
        if (file.endsWith('git-queue.sh') || (file === 'git' && args[0] === 'rev-parse')) {
          if (opts.cwd) cwds.push(opts.cwd);
        }
        if (file.endsWith('git-queue.sh') && args[0] === 'commit') return { stdout: '[kade/2750 abcd] kade: m\n', stderr: '' };
        if (file.endsWith('git-queue.sh') && args[0] === 'push') return { stdout: 'pushed\n', stderr: '' };
        if (file === 'git' && args[0] === 'rev-parse') return { stdout: 'kade/2750\n', stderr: '' };
        if (file === 'gh' && args[1] === 'view') { const e = new Error('no PR') as { code?: number }; e.code = 1; throw e; }
        if (file === 'gh' && args[1] === 'create') return { stdout: 'https://x/pr/1\n', stderr: '' };
        if (file === 'gh' && args[1] === 'merge') return { stdout: 'merged\n', stderr: '' };
        if (file.endsWith('cards') && args[0] === 'done') return { stdout: 'Done: #2750\n', stderr: '' };
        return { stdout: '', stderr: '' };
      });
      const server = buildMcpServer(() => 'kade', {
        boardReader: (async () => ({ ok: true, cards: oneCard })) as never,
        emitSpineEvent: (() => {}) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/platform/scripts/git-queue.sh',
        resolveWorkingTree: ((_role: 'kade') => '/fake/chorus-werk/kade') as never,
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      await handler({ method: 'tools/call', params: { name: 'chorus_acp', arguments: { role: 'kade' } } }, {});

      cwds.forEach((cwd) => expect(cwd).toBe('/fake/chorus-werk/kade'));
    });
  });
});
