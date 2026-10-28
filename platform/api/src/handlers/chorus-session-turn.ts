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

export async function handleSessionTurnPush(
  req: SessionTurnPushRequest,
  deps: SessionTurnDeps,
): Promise<SessionTurnDecision> {
  const rawCred = parseBearer(req.authorization);
  if (!rawCred) {
    return {
      status: 401,
      body: {
        error: 'authn-missing',
        message: 'POST /api/chorus/session/turns requires Authorization: Bearer <es256-jwt> (#4779)',
      },
    };
  }

  const v = await deps.verify(rawCred);
  if (!v.ok) {
    return {
      status: 401,
      body: {
        error: 'authn-missing',
        reason: v.reason,
        message: `credential verification failed: ${v.reason} (#4779)`,
      },
    };
  }

  const heldRole = await deps.roleForWebId(v.webId);
  if (!heldRole) {
    return {
      status: 403,
      body: {
        error: 'no-role-held',
        webId: v.webId,
        message: `principal ${v.webId} holds no active role (#4779)`,
      },
    };
  }

  const body = req.body ?? {};
  if (body.role && body.role !== heldRole) {
    return {
      status: 403,
      body: {
        error: 'not-your-lane',
        caller: heldRole,
        requested: body.role,
        message: `role '${heldRole}' may push only its own session turns, not '${body.role}' (#4779)`,
      },
    };
  }

  const turns = Array.isArray(body.turns) ? body.turns : [];
  const channel = `session:${heldRole}`;
  const defaultSource = (body.source || 'claude').trim() || 'claude';
  const defaultSessionId = body.session_id || null;

  const insertStmt = deps.db.prepare(
    `INSERT OR IGNORE INTO messages
      (source, source_id, channel, role, author, content, timestamp, session_id)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?)`,
  );

  let inserted = 0;
  const nowIso = new Date().toISOString();

  for (const turn of turns) {
    const content = typeof turn.content === 'string' ? turn.content.trim() : '';
    if (!content) continue;
    const author = turn.author === 'user' ? 'user' : 'assistant';
    const role = author === 'user' ? 'jeff' : heldRole;
    const source = (turn.source || defaultSource).trim() || 'claude';
    const timestamp = turn.timestamp || nowIso;
    const sessionId = turn.session_id || defaultSessionId;
    const digest = createHash('sha256').update(`${heldRole}:${author}:${timestamp}:${content}`).digest('hex').slice(0, 16);
    const sourceId = turn.source_id || `${source}:${sessionId || heldRole}:${digest}`;

    const info = insertStmt.run(source, sourceId, channel, role, author, content, timestamp, sessionId);
    if (info.changes > 0) inserted += 1;
  }

  if (inserted > 0) {
    try {
      deps.db
        .prepare(
          `INSERT INTO watermarks (source, last_seen, last_indexed)
           VALUES (?, ?, ?)
           ON CONFLICT(source) DO UPDATE SET last_seen = excluded.last_seen, last_indexed = excluded.last_indexed`,
        )
        .run(defaultSource, nowIso, nowIso);
    } catch {
      // Optional watermark table in minimal test schemas
    }
  }

  return {
    status: 200,
    body: {
      status: 'ok',
      role: heldRole,
      channel,
      inserted,
    },
  };
}
