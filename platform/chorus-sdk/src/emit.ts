import * as fs from 'fs';
import * as path from 'path';
import * as crypto from 'crypto';

const LOG_FILE = path.resolve(__dirname, '../../logs/chorus.log');
const SCHEMA_FILE = path.resolve(__dirname, '../../../designing/schemas/spine-events.json');

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
function resolveAppName(options: EmitOptions, ctx: SpineContext | undefined): string {
  if (options.appName) return options.appName;
  if (!ctx) return 'chorus-sdk';
  return Object.keys(PRODUCT_MAP).find((k) => PRODUCT_MAP[k] === ctx.product) ?? 'chorus-sdk';
}

function contextFields(ctx: SpineContext | undefined, stage: string | null) {
  return {
    product: ctx?.product ?? null,
    version: ctx?.version ?? null,
    card: ctx?.card ?? null,
    value_stream: ctx?.product ?? null,
    value_stream_step: stage,
    domain: ctx?.domain ?? null,
    stream: stage ?? ctx?.stream ?? null,
  };
}

function errorFields(level: string, extra: Record<string, string>) {
  const isError = level === 'error';
  return {
    error: isError ? (extra.error ?? null) : null,
    stack: isError ? (extra.stack ?? null) : null,
  };
}

function buildSpineEntry(
  event: string,
  role: string,
  extra: Record<string, string>,
  options: EmitOptions,
): SpineEvent {
  const ctx = options.context;
  const caller = extractCallerInfo();
  const level = extra.level ?? 'info';
  const stage = STREAM_NAME[getEventVertebra(event) ?? ''] ?? null;

  return {
    timestamp: new Date().toISOString(),
    level,
    appName: resolveAppName(options, ctx),
    component: options.component ?? 'sdk',
    event,
    role,
    ...contextFields(ctx, stage),
    trace_id: extra.trace_id ?? crypto.randomUUID(),
    file: null,
    function: caller.function,
    line: caller.line,
    ...errorFields(level, extra),
    ...extra,
  };
}

function buildTracePayload(entry: SpineEvent, extra: Record<string, string>, hopNum: number) {
  return {
    correlationId: entry.trace_id,
    hop: hopNum,
    callStack: extra.callStack || 'integration',
    source: {
      domain: extra.domain || entry.domain || null,
      service: extra.source_service || entry.event,
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
}

function maybeFireTrace(entry: SpineEvent, extra: Record<string, string>): void {
  const hopNum = extra.hop ? parseInt(extra.hop, 10) : NaN;
  if (isNaN(hopNum)) return;
  fetch('http://localhost:3340/api/chorus/trace', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(buildTracePayload(entry, extra, hopNum)),
    signal: AbortSignal.timeout(3000),
  }).catch(() => {});
}

export function emit(
  event: string,
  role: string,
  extra: Record<string, string> = {},
  options: EmitOptions = {},
): SpineEvent {
  const entry = buildSpineEntry(event, role, extra, options);

  try {
    fs.appendFileSync(options.logFile ?? LOG_FILE, JSON.stringify(entry) + '\n');
  } catch { /* best effort */ }

  maybeFireTrace(entry, extra);
  return entry;
}
