// Diagnostic write handlers (extracted from server.ts for #2205 wave 20):
// - handleRcaCreate: POST /api/chorus/rca — validates + inserts rca + emits spine event.
// - handleTraceCreate: POST /api/chorus/trace — validates + inserts trace row.

interface Req { body: any }
interface Res {
  status: (s: number) => Res;
  json: (b: any) => Res | void;
}

export interface RcaCreateDeps {
  dbPath: string;
  DatabaseCtor: new (path: string) => {
    pragma: (s: string) => void;
    prepare: (sql: string) => { run: (...args: any[]) => { lastInsertRowid: any } };
    close: () => void;
  };
  ensureTable: () => void;
  appendFileSync: (path: string, data: string) => void;
  chorusLogPath: string;
  now: () => string;
}

export function handleRcaCreate(req: Req, res: Res, deps: RcaCreateDeps): void {
  deps.ensureTable();
  const {
    title, trigger, timeline, root_cause,
    contributing_factors, corrective_actions, cards, spine_events,
  } = req.body || {};
  if (!title || !trigger || !root_cause) {
    res.status(400).json!({ error: 'title, trigger, and root_cause are required' });
    return;
  }
  const validStatuses = ['open', 'verified', 'closed'];
  const status = validStatuses.includes(req.body.status) ? req.body.status : 'open';
  const now = deps.now();
  const db = new deps.DatabaseCtor(deps.dbPath);
  db.pragma('journal_mode = WAL');
  const result = db.prepare(`
    INSERT INTO rcas (title, trigger_event, timeline, root_cause, contributing_factors, corrective_actions, cards, spine_events, status, created_at, updated_at)
    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
  `).run(
    title,
    trigger,
    timeline || '',
    root_cause,
    JSON.stringify(contributing_factors || []),
    JSON.stringify(corrective_actions || []),
    JSON.stringify(cards || []),
    JSON.stringify(spine_events || []),
    status,
    now,
    now,
  );
  db.close();

  deps.appendFileSync(deps.chorusLogPath, JSON.stringify({
    timestamp: now,
    level: 'info',
    appName: 'chorus-events',
    component: 'rca',
    event: 'rca.created',
    role: 'system',
    rca_id: String(result.lastInsertRowid),
    cards: JSON.stringify(cards || []),
  }) + '\n');

  res.json!({ ok: true, id: result.lastInsertRowid, status });
}

export interface TraceCreateDeps {
  dbPath: string;
  DatabaseCtor: new (path: string) => {
    pragma: (s: string) => void;
    prepare: (sql: string) => { run: (...args: any[]) => any };
    close: () => void;
  };
  ensureTable: () => void;
  now: () => string;
}

interface EndpointRef { domain?: string; service?: string; instance?: string }
interface ErrorInfo { classification?: string; message?: string }

function traceRowFromBody(body: any, now: string): unknown[] {
  const { correlationId, hop, callStack, source, destination, latencyMs, error } = body;
  const src: EndpointRef = source || {};
  const dst: EndpointRef = destination || {};
  const err: ErrorInfo = error || {};
  return [
    correlationId, hop, callStack,
    src.domain || null, src.service || null, src.instance || null,
    dst.domain || null, dst.service || null, dst.instance || null,
    now,
    latencyMs || null,
    err.classification || null, err.message || null,
    now,
  ];
}

export function handleTraceCreate(req: Req, res: Res, deps: TraceCreateDeps): void {
  deps.ensureTable();
  const body = req.body || {};
  if (!body.correlationId || !body.hop || !body.callStack) {
    res.status(400).json!({ error: 'correlationId, hop, and callStack are required' });
    return;
  }
  const now = deps.now();
  const db = new deps.DatabaseCtor(deps.dbPath);
  db.pragma('journal_mode = WAL');
  db.prepare(`
    INSERT INTO traces (correlation_id, hop, call_stack, source_domain, source_service, source_instance, dest_domain, dest_service, dest_instance, timestamp, latency_ms, error_class, error_message, created_at)
    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
  `).run(...traceRowFromBody(body, now));
  db.close();
  res.json!({ ok: true });
}
