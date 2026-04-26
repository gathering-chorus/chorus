import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { emit } from '../src/emit';

describe('emit', () => {
  const tmpFile = path.join(os.tmpdir(), `chorus-sdk-test-${Date.now()}.log`);

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
    expect(lines.length).toBeGreaterThanOrEqual(3);
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
