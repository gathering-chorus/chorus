/**
 * #1817 — Normalize log metadata
 * Tests for enriched spine event fields: product, stream, card, file, version,
 * function, line, error, stack + null handling + field name normalization.
 */
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { emit, createSpineContext, type SpineContext } from '../src/emit';

describe('spine event metadata enrichment (#1817)', () => {
  const tmpFile = path.join(os.tmpdir(), `chorus-sdk-metadata-test-${Date.now()}.log`);

  afterAll(() => {
    try { fs.unlinkSync(tmpFile); } catch { /* ignore */ }
  });

  // ── AC1: context injection at construction, not per-call ──

  describe('SpineContext construction', () => {
    it('createSpineContext returns context with product and version', () => {
      const ctx = createSpineContext({ appName: 'cards', version: 'abc1234' });
      expect(ctx.product).toBe('Chorus');
      expect(ctx.version).toBe('abc1234');
    });

    it('emit with context injects product and version into event', () => {
      const ctx = createSpineContext({ appName: 'cards', version: 'def5678' });
      const event = emit('test.enriched', 'kade', {}, { logFile: tmpFile, context: ctx });

      expect(event.product).toBe('Chorus');
      expect(event.version).toBe('def5678');
    });

    it('context fields appear in logged JSON line', () => {
      const ctx = createSpineContext({ appName: 'cards', version: 'ghi9012' });
      emit('test.logged', 'silas', {}, { logFile: tmpFile, context: ctx });

      const lines = fs.readFileSync(tmpFile, 'utf-8').trim().split('\n');
      const last = JSON.parse(lines[lines.length - 1]);
      expect(last.product).toBe('Chorus');
      expect(last.version).toBe('ghi9012');
    });
  });

  // ── AC2: field name normalization ──

  describe('field name normalization', () => {
    it('maps chorus-events appName to Chorus product', () => {
      const ctx = createSpineContext({ appName: 'chorus-events' });
      expect(ctx.product).toBe('Chorus');
    });

    it('maps cards appName to Chorus product', () => {
      const ctx = createSpineContext({ appName: 'cards' });
      expect(ctx.product).toBe('Chorus');
    });

    it('maps jeff-bridwell-personal-site appName to Gathering product', () => {
      const ctx = createSpineContext({ appName: 'jeff-bridwell-personal-site' });
      expect(ctx.product).toBe('Gathering');
    });

    it('maps unknown appName to the raw value', () => {
      const ctx = createSpineContext({ appName: 'something-else' });
      expect(ctx.product).toBe('something-else');
    });
  });

  // ── AC3: card from context ──

  describe('card field', () => {
    it('injects card from context', () => {
      const ctx = createSpineContext({ appName: 'cards', card: '1817' });
      const event = emit('test.card', 'kade', {}, { logFile: tmpFile, context: ctx });
      expect(event.card).toBe('1817');
    });

    it('per-call card overrides context card', () => {
      const ctx = createSpineContext({ appName: 'cards', card: '1817' });
      const event = emit('test.card.override', 'kade', { card: '9999' }, { logFile: tmpFile, context: ctx });
      expect(event.card).toBe('9999');
    });
  });

  // ── AC4: version from context ──

  describe('version field', () => {
    it('version injected from context', () => {
      const ctx = createSpineContext({ appName: 'cards', version: 'a1b2c3d' });
      const event = emit('test.version', 'kade', {}, { logFile: tmpFile, context: ctx });
      expect(event.version).toBe('a1b2c3d');
    });
  });

  // ── AC5: file from caller ──

  describe('file field', () => {
    it('file passed by caller appears in event', () => {
      const event = emit('test.file', 'kade', { file: 'src/handlers/seed.handler.ts' }, { logFile: tmpFile });
      expect(event.file).toBe('src/handlers/seed.handler.ts');
    });
  });

  // ── AC6: missing keys = null ──

  describe('null for missing fields', () => {
    it('emits null for fields not provided', () => {
      const ctx = createSpineContext({ appName: 'cards' });
      const event = emit('test.nulls', 'kade', {}, { logFile: tmpFile, context: ctx });

      // version not set in context → null
      expect(event.version).toBeNull();
      // card not set → null
      expect(event.card).toBeNull();
      // file not passed → null
      expect(event.file).toBeNull();
      // stream not set → null
      expect(event.stream).toBeNull();
    });

    it('null fields appear in JSON output', () => {
      const ctx = createSpineContext({ appName: 'cards' });
      emit('test.nulls.json', 'kade', {}, { logFile: tmpFile, context: ctx });

      const lines = fs.readFileSync(tmpFile, 'utf-8').trim().split('\n');
      const last = JSON.parse(lines[lines.length - 1]);
      expect(last.version).toBeNull();
      expect(last.card).toBeNull();
      expect(last.file).toBeNull();
    });
  });

  // ── AC (Jeff comment): function and line auto-populated ──

  describe('caller info auto-population', () => {
    it('function field is auto-populated from caller', () => {
      const ctx = createSpineContext({ appName: 'cards' });
      const event = emit('test.caller', 'kade', {}, { logFile: tmpFile, context: ctx });

      // Should contain something — the exact value depends on the call site
      expect(event.function).toBeDefined();
      expect(typeof event.function === 'string' || event.function === null).toBe(true);
    });

    it('line field is auto-populated from caller', () => {
      const ctx = createSpineContext({ appName: 'cards' });
      const event = emit('test.line', 'kade', {}, { logFile: tmpFile, context: ctx });

      expect(event.line).toBeDefined();
      expect(typeof event.line === 'string' || event.line === null).toBe(true);
    });
  });

  // ── AC (Jeff comment): error and stack on error events ──

  describe('error and stack fields', () => {
    it('error field included when level=error', () => {
      const ctx = createSpineContext({ appName: 'cards' });
      const event = emit('test.error', 'kade', {
        level: 'error',
        error: 'Fuseki unreachable',
        stack: 'Error: Fuseki unreachable\n    at handler.ts:42',
      }, { logFile: tmpFile, context: ctx });

      expect(event.error).toBe('Fuseki unreachable');
      expect(event.stack).toBe('Error: Fuseki unreachable\n    at handler.ts:42');
    });

    it('error and stack are null when not error level', () => {
      const ctx = createSpineContext({ appName: 'cards' });
      const event = emit('test.info', 'kade', {}, { logFile: tmpFile, context: ctx });

      expect(event.error).toBeNull();
      expect(event.stack).toBeNull();
    });
  });

  // ── Value stream auto-derivation ──

  describe('stream and stream_step auto-derived from event', () => {
    it('seed.received: value_stream=Chorus, value_stream_step=Capturing', () => {
      const ctx = createSpineContext({ appName: 'chorus-events' });
      const event = emit('seed.received', 'system', {}, { logFile: tmpFile, context: ctx });
      expect(event.value_stream).toBe('Chorus');
      expect(event.value_stream_step).toBe('Capturing');
      expect(event.stream).toBe('Capturing');
      // event name is already in event field — no stream_step needed
    });

    it('card.pulled: value_stream=Chorus, value_stream_step=Building', () => {
      const ctx = createSpineContext({ appName: 'cards' });
      const event = emit('card.pulled', 'kade', {}, { logFile: tmpFile, context: ctx });
      expect(event.value_stream).toBe('Chorus');
      expect(event.value_stream_step).toBe('Building');
      expect(event.stream).toBe('Building');
      expect(event.event).toBe('card.pulled');
    });

    it('card.accepted: value_stream_step=Proving', () => {
      const ctx = createSpineContext({ appName: 'cards' });
      const event = emit('card.accepted', 'kade', {}, { logFile: tmpFile, context: ctx });
      expect(event.value_stream_step).toBe('Proving');
    });

    it('Gathering product: value_stream=Gathering', () => {
      const ctx = createSpineContext({ appName: 'jeff-bridwell-personal-site' });
      const event = emit('seed.received', 'system', {}, { logFile: tmpFile, context: ctx });
      expect(event.value_stream).toBe('Gathering');
      expect(event.value_stream_step).toBe('Capturing');
    });

    it('unknown event gets null value_stream_step', () => {
      const ctx = createSpineContext({ appName: 'cards' });
      const event = emit('totally.unknown.event', 'kade', {}, { logFile: tmpFile, context: ctx });
      expect(event.value_stream_step).toBeNull();
      expect(event.event).toBe('totally.unknown.event');
    });

    it('value_stream fields appear in JSON output', () => {
      const ctx = createSpineContext({ appName: 'chorus-events' });
      emit('seed.routed', 'system', {}, { logFile: tmpFile, context: ctx });
      const lines = fs.readFileSync(tmpFile, 'utf-8').trim().split('\n');
      const last = JSON.parse(lines[lines.length - 1]);
      expect(last.value_stream).toBe('Chorus');
      expect(last.value_stream_step).toBe('Capturing');
      expect(last.stream).toBe('Capturing');
      expect(last.event).toBe('seed.routed');
    });
  });

  // ── trace_id correlation UUID ──

  describe('trace_id correlation', () => {
    it('auto-generates trace_id when not passed', () => {
      const ctx = createSpineContext({ appName: 'cards' });
      const event = emit('test.trace', 'kade', {}, { logFile: tmpFile, context: ctx });
      expect(event.trace_id).toBeDefined();
      expect(typeof event.trace_id).toBe('string');
      expect(event.trace_id!.length).toBeGreaterThan(0);
    });

    it('each emit gets a unique trace_id', () => {
      const ctx = createSpineContext({ appName: 'cards' });
      const e1 = emit('test.trace1', 'kade', {}, { logFile: tmpFile, context: ctx });
      const e2 = emit('test.trace2', 'kade', {}, { logFile: tmpFile, context: ctx });
      expect(e1.trace_id).not.toBe(e2.trace_id);
    });

    it('caller can pass trace_id to continue a trace', () => {
      const ctx = createSpineContext({ appName: 'cards' });
      const e1 = emit('seed.received', 'system', {}, { logFile: tmpFile, context: ctx });
      const traceId = e1.trace_id!;
      const e2 = emit('seed.routed', 'system', { trace_id: traceId }, { logFile: tmpFile, context: ctx });
      expect(e2.trace_id).toBe(traceId);
    });

    it('trace_id appears in JSON output', () => {
      const ctx = createSpineContext({ appName: 'cards' });
      emit('test.trace.json', 'kade', {}, { logFile: tmpFile, context: ctx });
      const lines = fs.readFileSync(tmpFile, 'utf-8').trim().split('\n');
      const last = JSON.parse(lines[lines.length - 1]);
      expect(last.trace_id).toBeDefined();
      expect(last.trace_id.length).toBeGreaterThan(0);
    });
  });

  // ── Backward compatibility ──

  describe('backward compatibility', () => {
    it('emit without context still works (existing callers unaffected)', () => {
      const event = emit('test.compat', 'wren', { card_id: '500' }, { logFile: tmpFile });

      expect(event.event).toBe('test.compat');
      expect(event.role).toBe('wren');
      expect(event.card_id).toBe('500');
      expect(event.appName).toBe('chorus-sdk');
    });

    it('existing SpineEvent fields preserved', () => {
      const event = emit('card.accepted', 'silas', { card_id: '177' }, {
        logFile: tmpFile,
        appName: 'cards',
        component: 'cli',
      });

      expect(event.timestamp).toBeTruthy();
      expect(event.level).toBe('info');
      expect(event.appName).toBe('cards');
      expect(event.component).toBe('cli');
    });
  });
});
