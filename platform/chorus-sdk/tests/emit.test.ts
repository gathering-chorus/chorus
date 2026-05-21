import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { emit } from '../src/emit';

describe('emit', () => {
  const tmpFile = path.join(os.tmpdir(), `chorus-sdk-test-${Date.now()}.log`);

  beforeEach(() => {
    try { fs.writeFileSync(tmpFile, ''); } catch { /* ignore */ }
  });

  afterAll(() => {
    try { fs.unlinkSync(tmpFile); } catch { /* ignore */ }
  });

  it('writes a JSON line to the log file', () => {
    const event = emit('test.event', 'silas', { card: '972' }, { logFile: tmpFile });

    expect(event.event).toBe('test.event');
    expect(event.role).toBe('silas');
    expect(event.card).toBe('972');
    expect(event.appName).toBe('chorus-sdk');
    expect(event.timestamp).toBeTruthy();

    const lines = fs.readFileSync(tmpFile, 'utf-8').trim().split('\n');
    expect(lines).toHaveLength(1);
    const parsed = JSON.parse(lines[0]);
    expect(parsed.event).toBe('test.event');
    expect(parsed.role).toBe('silas');
  });

  it('appends multiple events', () => {
    emit('event.one', 'wren', {}, { logFile: tmpFile });
    emit('event.two', 'kade', {}, { logFile: tmpFile });

    const lines = fs.readFileSync(tmpFile, 'utf-8').trim().split('\n');
    expect(lines).toHaveLength(2);
  });

  it('uses custom appName and component', () => {
    const event = emit('custom.event', 'silas', {}, {
      logFile: tmpFile,
      appName: 'cards',
      component: 'cli',
    });

    expect(event.appName).toBe('cards');
    expect(event.component).toBe('cli');
  });

  it('is backward compatible with cards format', () => {
    emit('card.accepted', 'silas', { card_id: '177' }, { logFile: tmpFile });
    const lines = fs.readFileSync(tmpFile, 'utf-8').trim().split('\n');
    const last = JSON.parse(lines[lines.length - 1]);

    expect(last).toHaveProperty('timestamp');
    expect(last).toHaveProperty('level', 'info');
    expect(last).toHaveProperty('appName');
    expect(last).toHaveProperty('component');
    expect(last).toHaveProperty('event', 'card.accepted');
    expect(last).toHaveProperty('role', 'silas');
    expect(last).toHaveProperty('card_id', '177');
  });

  describe('#3023 — trace_id + branch env-fallback (TS twin of the shim-wrapper bridge)', () => {
    const saved = { trace: process.env.CHORUS_TRACE_ID, branch: process.env.CHORUS_BRANCH };
    afterEach(() => {
      if (saved.trace === undefined) delete process.env.CHORUS_TRACE_ID; else process.env.CHORUS_TRACE_ID = saved.trace;
      if (saved.branch === undefined) delete process.env.CHORUS_BRANCH; else process.env.CHORUS_BRANCH = saved.branch;
    });

    it('AC1: uses CHORUS_TRACE_ID from env when extra has none — so a multi-call action shares one trace', () => {
      process.env.CHORUS_TRACE_ID = '019e4a00-aaaa-7000-8000-000000000001';
      const a = emit('card.demo.started', 'kade', {}, { logFile: tmpFile });
      const b = emit('card.item.commented', 'kade', {}, { logFile: tmpFile });
      expect(a.trace_id).toBe('019e4a00-aaaa-7000-8000-000000000001');
      expect(b.trace_id).toBe(a.trace_id); // both emits of the demo action link
    });

    it('explicit extra.trace_id still wins over env', () => {
      process.env.CHORUS_TRACE_ID = '019e4a00-aaaa-7000-8000-000000000001';
      const e = emit('card.demo.started', 'kade', { trace_id: 'explicit-trace' }, { logFile: tmpFile });
      expect(e.trace_id).toBe('explicit-trace');
    });

    it('mints a random trace when neither extra nor env is set (unchanged behavior)', () => {
      delete process.env.CHORUS_TRACE_ID;
      const e = emit('card.demo.started', 'kade', {}, { logFile: tmpFile });
      expect(typeof e.trace_id).toBe('string');
      expect(e.trace_id).not.toBe('explicit-trace');
    });

    it('AC1: reads /tmp/demo-trace-<card>.txt when no extra/env trace, so cards-CLI demo emits link (matches chorus_log.rs #2897)', () => {
      const savedTrace = process.env.CHORUS_TRACE_ID;
      delete process.env.CHORUS_TRACE_ID;
      const card = 999301;
      const tracePath = `/tmp/demo-trace-${card}.txt`;
      fs.writeFileSync(tracePath, '019e4b00-dddd-7000-8000-000000000abc\n');
      try {
        const a = emit('card.demo.started', 'kade', { card_id: card }, { logFile: tmpFile });
        const b = emit('card.item.commented', 'kade', { card_id: card }, { logFile: tmpFile });
        expect(a.trace_id).toBe('019e4b00-dddd-7000-8000-000000000abc');
        expect(b.trace_id).toBe(a.trace_id); // both demo emits share the file's trace
      } finally {
        try { fs.unlinkSync(tracePath); } catch { /* ignore */ }
        if (savedTrace === undefined) delete process.env.CHORUS_TRACE_ID;
        else process.env.CHORUS_TRACE_ID = savedTrace;
      }
    });

    it('AC3: stamps branch from env on git/werk events (card.*)', () => {
      process.env.CHORUS_BRANCH = 'kade/3023';
      const e = emit('card.demo.started', 'kade', {}, { logFile: tmpFile }) as Record<string, unknown>;
      expect(e.branch).toBe('kade/3023');
    });

    it('AC3: does NOT stamp branch on non-git events (matches shim-wrapper MUST-carry gate)', () => {
      process.env.CHORUS_BRANCH = 'kade/3023';
      const e = emit('library.health.checked', 'kade', {}, { logFile: tmpFile }) as Record<string, unknown>;
      expect(e.branch).toBeUndefined();
    });
  });

  describe('trace hop bridge (#2100, ADR-024)', () => {
    let originalFetch: typeof global.fetch;
    let fetchCalls: Array<{ url: string; body: unknown }>;

    beforeEach(() => {
      originalFetch = global.fetch;
      fetchCalls = [];
      global.fetch = ((url: string, init?: { body?: string }) => {
        fetchCalls.push({ url, body: init?.body ? JSON.parse(init.body) : undefined });
        return Promise.resolve({ ok: true, status: 200 } as Response);
      }) as typeof global.fetch;
    });

    afterEach(() => {
      global.fetch = originalFetch;
    });

    it('posts trace payload when extra.hop is provided', () => {
      emit('card.pulled', 'kade', {
        hop: '1',
        callStack: 'integration',
        source_service: 'cards',
        dest_service: 'chorus-api',
        domain: 'chorus',
        latencyMs: '42',
      }, { logFile: tmpFile });

      expect(fetchCalls).toHaveLength(1);
      expect(fetchCalls[0].url).toContain('/api/chorus/trace');
      const payload = fetchCalls[0].body as Record<string, unknown>;
      expect(payload.hop).toBe(1);
      expect(payload.callStack).toBe('integration');
      expect(payload.latencyMs).toBe(42);
      expect(payload.source).toMatchObject({ service: 'cards', domain: 'chorus' });
      expect(payload.destination).toMatchObject({ service: 'chorus-api' });
    });

    it('includes error classification when extra.error_class present', () => {
      emit('card.failed', 'kade', {
        hop: '2',
        error_class: 'transient',
        error_message: 'temporary db timeout',
      }, { logFile: tmpFile });

      const payload = fetchCalls[0].body as Record<string, unknown>;
      expect(payload.error).toMatchObject({
        classification: 'transient',
        message: 'temporary db timeout',
        retryable: true,
      });
    });

    it('does not post trace when hop is missing', () => {
      emit('card.pulled', 'kade', { role: 'kade' }, { logFile: tmpFile });
      expect(fetchCalls).toHaveLength(0);
    });

    it('does not post trace when hop is non-numeric', () => {
      emit('card.pulled', 'kade', { hop: 'NaN' }, { logFile: tmpFile });
      expect(fetchCalls).toHaveLength(0);
    });
  });
});
