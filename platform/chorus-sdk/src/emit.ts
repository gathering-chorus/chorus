/* eslint-disable security/detect-non-literal-fs-filename, security/detect-object-injection --
 * Server-controlled CHORUS_LOG path; payload indexing on typed event keys.
 */
import * as fs from 'fs';
import * as path from 'path';
import * as crypto from 'crypto';

const LOG_FILE = path.resolve(__dirname, '../../logs/chorus.log');
const SCHEMA_FILE = path.resolve(__dirname, '../../../designing/schemas/spine-events.json');

// ── Event → vertebra lookup from spine schema (lazy loaded) ──

let eventVertebra: Record<string, string | null> | null = null;

function getEventVertebra(event: string): string | null {
  if (!eventVertebra) {
    try {
      const schema = JSON.parse(fs.readFileSync(SCHEMA_FILE, 'utf-8'));
      eventVertebra = {};
      for (const [name, info] of Object.entries(schema.events || {})) {
        eventVertebra[name] = (info as { vertebra?: string }).vertebra ?? null;
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
  // #2876: card_id is a number when present (canonical match with MCP-side
  // emitters). Other dynamic keys remain string|null.
  [key: string]: string | number | null;
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

function errorFields(level: string, extra: Partial<Record<string, string | number>>) {
  const isError = level === 'error';
  const errStr = typeof extra.error === 'string' ? extra.error : null;
  const stackStr = typeof extra.stack === 'string' ? extra.stack : null;
  return {
    error: isError ? errStr : null,
    stack: isError ? stackStr : null,
  };
}

// #3121 / ADR-032 §3 — per-card-run trace carrier. Read the canonical /tmp/<card>-trace,
// fall back to the legacy /tmp/demo-trace-<card>.txt (back-compat until werk-demo +
// chorus_log.rs migrate to the one filename), else MINT-AND-PERSIST to /tmp/<card>-trace
// so every emit of one card-operation shares one trace instead of each minting its own
// (the #3119 fragmentation: setCard fires 4-5 emits in one process). Best-effort write —
// a failed persist never blocks the emit. acp cleans the carrier (Silas follow-on).
function resolveCardTrace(card: string): string {
  const fs = require('fs') as typeof import('fs');
  const carrier = `/tmp/${card}-trace`;
  for (const p of [carrier, `/tmp/demo-trace-${card}.txt`]) {
    try {
      const t = fs.readFileSync(p, 'utf-8').trim();
      if (t) return t;
    } catch { /* not present — try next, else mint */ }
  }
  const minted = crypto.randomUUID();
  try { fs.writeFileSync(carrier, `${minted}\n`); } catch { /* best-effort; never block emit */ }
  return minted;
}

// #3023 trace precedence: explicit extra.trace_id > CHORUS_TRACE_ID env >
// per-card carrier (read-or-mint-and-persist, #3121/ADR-032 §3) > random. Mirrors
// chorus_log.rs (#2897) so the TS + Rust emitters of one action share a trace.
// Split out of buildSpineEntry to hold it under the complexity ceiling.
function resolveTraceId(
  extra: Partial<Record<string, string | number>>,
  envTrace: string | undefined,
): string {
  if (typeof extra.trace_id === 'string') return extra.trace_id;
  if (envTrace && envTrace.length > 0) return envTrace;
  const card = typeof extra.card_id === 'number'
    ? String(extra.card_id)
    : (typeof extra.card_id === 'string' && /^\d+$/.test(extra.card_id) ? extra.card_id : null);
  if (card) return resolveCardTrace(card);
  return crypto.randomUUID();
}

function buildSpineEntry(
  event: string,
  role: string,
  extra: Partial<Record<string, string | number>>,
  options: EmitOptions,
): SpineEvent {
  const ctx = options.context;
  const caller = extractCallerInfo();
  const level = typeof extra.level === 'string' ? extra.level : 'info';
  const stage = STREAM_NAME[getEventVertebra(event) ?? ''] ?? null;

  // #3023 — env-fallback for the two cross-process observability keys, the TS
  // twin of the shim-wrapper bridge (#2857). Precedence: explicit extra > env >
  // (trace_id) random. Lets a multi-call action (a /demo, a build pipeline) set
  // CHORUS_TRACE_ID once and have every emit from that process tree share one
  // trace, instead of each emit minting its own.
  // #3023 — trace precedence mirrors chorus_log.rs (#2897) exactly so the TS
  // (cards-CLI) and Rust (chorus-log) emitters of one action share a trace:
  // explicit extra > CHORUS_TRACE_ID env > /tmp/demo-trace-<card>.txt > random.
  // The demo-trace file is written by demo_preflight on /demo entry and read
  // here keyed on card_id — the cards-CLI demo emits (card.demo.started, gate
  // comments) carry card_id, so they now link into the same demo trace instead
  // of each minting its own.
  const trace_id = resolveTraceId(extra, process.env.CHORUS_TRACE_ID);

  const entry: SpineEvent = {
    timestamp: new Date().toISOString(),
    level,
    appName: resolveAppName(options, ctx),
    component: options.component ?? 'sdk',
    event,
    role,
    ...contextFields(ctx, stage),
    trace_id,
    file: null,
    function: caller.function,
    line: caller.line,
    ...errorFields(level, extra),
    ...extra,
  };

  // branch = the git surface work ran on (third key). Stamp from env, gated to
  // the same git/werk MUST-carry prefixes as shim-wrapper.sh
  // (chorus_*|build.*|deploy.*|card.*) so non-git events (health, nudge) don't
  // carry it. Explicit extra.branch (already in `entry` via the spread above)
  // wins, so only set when the caller didn't.
  const envBranch = process.env.CHORUS_BRANCH;
  if (envBranch && typeof extra.branch !== 'string' && /^(chorus_|build\.|deploy\.|card\.)/.test(event)) {
    entry.branch = envBranch;
  }
  return entry;
}

function buildTracePayload(entry: SpineEvent, extra: Partial<Record<string, string | number>>, hopNum: number) {
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
    latencyMs: extra.latencyMs ? parseInt(String(extra.latencyMs), 10) : undefined,
    error: extra.error_class ? {
      classification: extra.error_class,
      message: extra.error_message || '',
      retryable: extra.error_class === 'transient',
    } : undefined,
  };
}

function maybeFireTrace(entry: SpineEvent, extra: Partial<Record<string, string | number>>): void {
  const hopNum = extra.hop ? parseInt(String(extra.hop), 10) : NaN;
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
  extra: Partial<Record<string, string | number>> = {},
  options: EmitOptions = {},
): SpineEvent {
  const entry = buildSpineEntry(event, role, extra, options);

  try {
    fs.appendFileSync(options.logFile ?? LOG_FILE, JSON.stringify(entry) + '\n');
  } catch { /* best effort */ }

  maybeFireTrace(entry, extra);
  return entry;
}
