// Spine-event write handler (extracted from server.ts for #2205 wave 21).
// POST /api/chorus/spine-event:
// - Validates event field.
// - Appends a spine-log line (best-effort, swallows disk errors).
// - If the body includes a numeric `hop`, also inserts a trace row
//   with synthesized correlation id when absent.

import type { Request as Req, Response as Res } from 'express';

/** Prepared-statement run — any retained due to better-sqlite3 Statement variance. */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
type RunFn = (...args: any[]) => unknown;

export interface SpineEventDeps {
  appendFileSync: typeof import('fs').appendFileSync;
  chorusLogPath: string;
  now: () => string;
  traceDbPath: string;
  DatabaseCtor: new (path: string) => {
    pragma: (s: string) => void;
    prepare: (sql: string) => { run: RunFn };
    close: () => void;
  };
  ensureTraceTable: () => void;
}

function buildSpineEntry(event: string, role: string | undefined, fields: Record<string, unknown>, now: number): Record<string, unknown> {
  return {
    timestamp: now,
    level: 'info',
    appName: 'chorus-events',
    component: 'spine-service',
    event,
    role: role || 'system',
    ...fields,
  };
}

function appendSpineLog(deps: SpineEventDeps, entry: Record<string, unknown>): void {
  try {
    deps.appendFileSync(deps.chorusLogPath, JSON.stringify(entry) + '\n');
  } catch {
    /* best-effort spine log; swallow */
  }
}

function insertTraceRow(deps: SpineEventDeps, event: string, fields: Record<string, unknown>): void {
  deps.ensureTraceTable();
  const db = new deps.DatabaseCtor(deps.traceDbPath);
  db.pragma('journal_mode = WAL');
  const ts = deps.now();
  db.prepare(`
    INSERT INTO traces (correlation_id, hop, call_stack, source_domain, source_service, source_instance, dest_domain, dest_service, dest_instance, timestamp, latency_ms, error_class, error_message, created_at)
    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
  `).run(
    fields.trace_id || `spine-${Date.now()}`,
    fields.hop,
    fields.callStack || 'integration',
    fields.domain || null,
    fields.source_service || event,
    fields.source_instance || null,
    fields.dest_domain || fields.domain || null,
    fields.dest_service || null,
    fields.dest_instance || null,
    ts,
    fields.latencyMs || null,
    fields.error_class || null,
    fields.error_message || null,
    ts,
  );
  db.close();
}

export function handleSpineEvent(req: Req, res: Res, deps: SpineEventDeps): void {
  const { event, role, ...fields } = req.body || {};
  if (!event) {
    res.status(400).json!({ error: 'event is required' });
    return;
  }
  appendSpineLog(deps, buildSpineEntry(event, role, fields, deps.now()));
  if (typeof fields.hop === 'number' && !isNaN(fields.hop)) {
    insertTraceRow(deps, event, fields);
  }
  res.json!({ ok: true });
}
