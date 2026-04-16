import * as fs from 'fs';
import * as path from 'path';
import * as crypto from 'crypto';

const LOG_FILE = path.resolve(__dirname, '../../logs/chorus.log');
const SCHEMA_FILE = path.resolve(__dirname, '../../schemas/spine-events.json');

// ── Event → vertebra lookup from spine schema (lazy loaded) ──

let eventVertebra: Record<string, string> | null = null;

function getEventVertebra(event: string): string | null {
  if (!eventVertebra) {
    try {
      const schema = JSON.parse(fs.readFileSync(SCHEMA_FILE, 'utf-8'));
      eventVertebra = {};
      for (const [name, info] of Object.entries(schema.events || {})) {
        eventVertebra[name] = (info as any).vertebra ?? null;
      }
    } catch {
      eventVertebra = {};
    }
  }
  return eventVertebra[event] ?? null;
}

// ── Vertebra → value stream name ──

const STREAM_NAME: Record<string, string> = {
  capturing: 'Capturing',
  directing: 'Directing',
  designing: 'Designing',
  building: 'Building',
  proving: 'Proving',
};

// ── Product name mapping (AC2: appName → product) ──

const PRODUCT_MAP: Record<string, string> = {
  'chorus-events': 'Chorus',
  'chorus-sdk': 'Chorus',
  'cards': 'Chorus',
  'jeff-bridwell-personal-site': 'Gathering',
};

export interface SpineEvent {
  timestamp: string;
  level: string;
  appName: string;
  component: string;
  event: string;
  role: string;
  [key: string]: string | null;
}

export interface SpineContext {
  product: string;
  version: string | null;
  card: string | null;
  stream: string | null;
  domain: string | null;
}

export interface EmitOptions {
  appName?: string;
  component?: string;
  logFile?: string;
  context?: SpineContext;
}

/**
 * Create a spine context — constructed once at startup, injected into every emit call.
 * AC1: context injection at construction, not per-call site.
 */
export function createSpineContext(opts: {
  appName: string;
  version?: string;
  card?: string;
  stream?: string;
  domain?: string;
}): SpineContext {
  return {
    product: PRODUCT_MAP[opts.appName] ?? opts.appName,
    version: opts.version ?? null,
    card: opts.card ?? null,
    stream: opts.stream ?? null,
    domain: opts.domain ?? null,
  };
}

/**
 * Extract caller function name and line number from stack trace.
 * Returns { function, line } or nulls if unavailable.
 */
function extractCallerInfo(): { function: string | null; line: string | null } {
  const stack = new Error().stack;
  if (!stack) return { function: null, line: null };

  // Stack frames: [0] Error, [1] extractCallerInfo, [2] emit, [3] actual caller
  const frames = stack.split('\n');
  const callerFrame = frames[3];
  if (!callerFrame) return { function: null, line: null };

  // Match "at functionName (file:line:col)" or "at file:line:col"
  const fnMatch = callerFrame.match(/at\s+(\S+)\s+\(/);
  const lineMatch = callerFrame.match(/:(\d+):\d+\)?$/);

  return {
    function: fnMatch ? fnMatch[1] : null,
    line: lineMatch ? lineMatch[1] : null,
  };
}

/**
 * Emit a spine event to chorus.log.
 * Backward compatible with the existing JSON-line format.
 * AC1: context fields injected automatically when context is provided.
 * AC6: missing keys emit as null, not error.
 */
export function emit(
  event: string,
  role: string,
  extra: Record<string, string> = {},
  options: EmitOptions = {},
): SpineEvent {
  const ctx = options.context;
  const caller = extractCallerInfo();
  const level = extra.level ?? 'info';

  const entry: SpineEvent = {
    timestamp: new Date().toISOString(),
    level,
    appName: options.appName ?? (ctx ? Object.keys(PRODUCT_MAP).find(k => PRODUCT_MAP[k] === ctx.product) ?? 'chorus-sdk' : 'chorus-sdk'),
    component: options.component ?? 'sdk',
    event,
    role,
    // AC1/AC2: enriched context fields
    product: ctx?.product ?? null,
    version: ctx?.version ?? null,
    card: ctx?.card ?? null,
    // Value stream = the product (Chorus or Gathering)
    value_stream: ctx?.product ?? null,
    // Value stream step = the Werk stage (Capturing, Directing, etc.)
    value_stream_step: STREAM_NAME[getEventVertebra(event) ?? ''] ?? null,
    // Correlation UUID — auto-generated if not passed, caller passes to continue a trace
    trace_id: extra.trace_id ?? crypto.randomUUID(),
    // Domain — from context (card tag) or caller
    domain: ctx?.domain ?? null,
    // Stream = Werk stage (same as value_stream_step)
    stream: STREAM_NAME[getEventVertebra(event) ?? ''] ?? ctx?.stream ?? null,
    // AC5/Jeff: file from caller, function/line auto-populated
    file: null,
    function: caller.function,
    line: caller.line,
    // Jeff: error/stack only on error level
    error: level === 'error' ? (extra.error ?? null) : null,
    stack: level === 'error' ? (extra.stack ?? null) : null,
    // Spread extra last — caller overrides defaults
    ...extra,
  };

  try {
    const target = options.logFile ?? LOG_FILE;
    fs.appendFileSync(target, JSON.stringify(entry) + '\n');
  } catch { /* best effort */ }

  // Trace hop bridge — fire-and-forget to chorus-api (#2100, ADR-024)
  const hopNum = extra.hop ? parseInt(extra.hop, 10) : undefined;
  if (hopNum !== undefined && !isNaN(hopNum)) {
    const tracePayload = {
      correlationId: entry.trace_id,
      hop: hopNum,
      callStack: extra.callStack || 'integration',
      source: {
        domain: extra.domain || entry.domain || null,
        service: extra.source_service || event,
        instance: extra.source_instance || null,
      },
      destination: extra.dest_service ? {
        domain: extra.dest_domain || extra.domain || entry.domain || null,
        service: extra.dest_service,
        instance: extra.dest_instance || null,
      } : undefined,
      latencyMs: extra.latencyMs ? parseInt(extra.latencyMs, 10) : undefined,
      error: extra.error_class ? {
        classification: extra.error_class,
        message: extra.error_message || '',
        retryable: extra.error_class === 'transient',
      } : undefined,
    };
    fetch('http://localhost:3340/api/chorus/trace', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(tracePayload),
      signal: AbortSignal.timeout(3000),
    }).catch(() => {}); // Silent — tracing never blocks
  }

  return entry;
}
