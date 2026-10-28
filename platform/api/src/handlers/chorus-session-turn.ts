// @domain: search — placed by chorus-crawl's domain rules (#4538)
/**
 * #4779 — POST /api/chorus/session/turns
 *
 * Verified session turn ingestion across macOS accounts and model runtimes.
 * Each role session pushes its own user and assistant turns authenticated by
 * its CSS ES256 credential (`Authorization: Bearer <jwt>`). The role and
 * `session:<role>` channel are derived strictly from the verified WebID ->
 * `holdsRole` projection (`roleForWebId`), never from folder names or an
 * unverified request field.
 */
import type Database from 'better-sqlite3';
import { createHash } from 'crypto';
import type { VerifyResult } from '../es256-identity';

export interface SessionTurnItem {
  author?: string;
  content?: string;
  timestamp?: string;
  source?: string;
  source_id?: string;
  session_id?: string;
}

export interface SessionTurnPushBody {
  role?: string;
  source?: string;
  session_id?: string;
  turns?: SessionTurnItem[];
}

export interface SessionTurnPushRequest {
  authorization?: string;
  body?: SessionTurnPushBody;
}

export interface SessionTurnDeps {
  db: Database.Database;
  verify: (cred: string) => Promise<VerifyResult>;
  roleForWebId: (webId: string) => Promise<string | null>;
}

export interface SessionTurnDecision {
  status: 200 | 400 | 401 | 403;
  body: Record<string, unknown>;
}

function parseBearer(h: string | undefined): string {
  if (!h) return '';
  const m = /^Bearer\s+(\S+)$/i.exec(h.trim());
  return m ? m[1] : '';
}

async function authenticateRole(
  req: SessionTurnPushRequest,
  deps: SessionTurnDeps,
): Promise<{ ok: true; role: string } | { ok: false; decision: SessionTurnDecision }> {
  const rawCred = parseBearer(req.authorization);
  if (!rawCred) {
    return {
      ok: false,
      decision: {
        status: 401,
        body: {
          error: 'authn-missing',
          message: 'POST /api/chorus/session/turns requires Authorization: Bearer <es256-jwt> (#4779)',
        },
      },
    };
  }
  const v = await deps.verify(rawCred);
  if (!v.ok) {
    return {
      ok: false,
      decision: {
        status: 401,
        body: {
          error: 'authn-missing',
          reason: v.reason,
          message: `credential verification failed: ${v.reason} (#4779)`,
        },
      },
    };
  }
  const heldRole = await deps.roleForWebId(v.webId);
  if (!heldRole) {
    return {
      ok: false,
      decision: {
        status: 403,
        body: {
          error: 'no-role-held',
          webId: v.webId,
          message: `principal ${v.webId} holds no active role (#4779)`,
        },
      },
    };
  }
  const claimed = req.body?.role;
  if (claimed && claimed !== heldRole) {
    return {
      ok: false,
      decision: {
        status: 403,
        body: {
          error: 'not-your-lane',
          caller: heldRole,
          requested: claimed,
          message: `role '${heldRole}' may push only its own session turns, not '${claimed}' (#4779)`,
        },
      },
    };
  }
  return { ok: true, role: heldRole };
}

interface NormalizedTurnRow {
  source: string;
  sourceId: string;
  channel: string;
  role: string;
  author: string;
  content: string;
  timestamp: string;
  sessionId: string | null;
}

function normalizeTurnRow(
  turn: SessionTurnItem,
  heldRole: string,
  defaultSource: string,
  defaultSessionId: string | null,
  nowIso: string,
): NormalizedTurnRow | null {
  const content = typeof turn.content === 'string' ? turn.content.trim() : '';
  if (!content) return null;
  const author = turn.author === 'user' ? 'user' : 'assistant';
  const role = author === 'user' ? 'jeff' : heldRole;
  const source = (turn.source || defaultSource).trim() || 'claude';
  const timestamp = turn.timestamp || nowIso;
  const sessionId = turn.session_id || defaultSessionId;
  const digest = createHash('sha256')
    .update(`${heldRole}:${author}:${timestamp}:${content}`)
    .digest('hex')
    .slice(0, 16);
  const sourceId = turn.source_id || `${source}:${sessionId || heldRole}:${digest}`;
  return {
    source,
    sourceId,
    channel: `session:${heldRole}`,
    role,
    author,
    content,
    timestamp,
    sessionId,
  };
}

function updateWatermarkBestEffort(db: Database.Database, source: string, nowIso: string): void {
  try {
    db.prepare(
      `INSERT INTO watermarks (source, last_seen, last_indexed)
       VALUES (?, ?, ?)
       ON CONFLICT(source) DO UPDATE SET last_seen = excluded.last_seen, last_indexed = excluded.last_indexed`,
    ).run(source, nowIso, nowIso);
  } catch {
    // Optional watermark table in minimal test schemas
  }
}

export async function handleSessionTurnPush(
  req: SessionTurnPushRequest,
  deps: SessionTurnDeps,
): Promise<SessionTurnDecision> {
  const auth = await authenticateRole(req, deps);
  if (!auth.ok) return auth.decision;

  const heldRole = auth.role;
  const body = req.body ?? {};
  const turns = Array.isArray(body.turns) ? body.turns : [];
  const defaultSource = (body.source || 'claude').trim() || 'claude';
  const defaultSessionId = body.session_id || null;
  const nowIso = new Date().toISOString();

  const insertStmt = deps.db.prepare(
    `INSERT OR IGNORE INTO messages
      (source, source_id, channel, role, author, content, timestamp, session_id)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?)`,
  );

  let inserted = 0;
  for (const turn of turns) {
    const row = normalizeTurnRow(turn, heldRole, defaultSource, defaultSessionId, nowIso);
    if (!row) continue;
    const info = insertStmt.run(
      row.source,
      row.sourceId,
      row.channel,
      row.role,
      row.author,
      row.content,
      row.timestamp,
      row.sessionId,
    );
    if (info.changes > 0) inserted += 1;
  }

  if (inserted > 0) {
    updateWatermarkBestEffort(deps.db, defaultSource, nowIso);
  }

  return {
    status: 200,
    body: {
      status: 'ok',
      role: heldRole,
      channel: `session:${heldRole}`,
      inserted,
    },
  };
}
