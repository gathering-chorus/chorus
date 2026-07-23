/**
 * #3667 — tunnel auth gate as a pure decision table.
 *
 * One function owns the remote-access policy so it is reviewable and
 * hermetically testable in one place. The boundary (agreed Kade/Silas,
 * origin Jeff's 2026-07-23 report):
 *
 *  - LOCAL requests: pass everything (unchanged from #1719).
 *  - REMOTE (cf-proxied) requests:
 *      - local-only paths (/health, /metrics, /api/debug): forbid, always.
 *      - admin prefixes (/api/restart, /api/commands/, /api/session/):
 *        forbid, always — token is never consulted (mutating/admin surface).
 *      - the READ PAIR (GET /api/stream, GET /api/flow) + the domain-detail
 *        proxy (GET /api/domain-detail/): token decides — valid → pass,
 *        missing/invalid → auth-required (401 login, NOT hard 403).
 *        Non-GET on these paths: forbid — only reads are opened.
 *      - everything else: token decides (the pre-existing #1719 behavior).
 */

export type GateOutcome = 'pass' | 'forbid' | 'auth-required';

const LOCAL_ONLY_PATHS = ['/health', '/metrics', '/api/debug'];
const ADMIN_PATH_PREFIXES = ['/api/restart', '/api/commands/', '/api/session/'];
const REMOTE_READ_PATHS = ['/api/stream', '/api/flow'];
const REMOTE_READ_PREFIXES = ['/api/domain-detail/'];

export function isRemoteReadPath(path: string): boolean {
  return REMOTE_READ_PATHS.includes(path)
    || REMOTE_READ_PREFIXES.some((p) => path.startsWith(p));
}

export function gateDecision(
  path: string,
  method: string,
  isLocalReq: boolean,
  tokenValid: boolean,
): GateOutcome {
  if (isLocalReq) return 'pass';
  if (LOCAL_ONLY_PATHS.includes(path)) return 'forbid';
  if (ADMIN_PATH_PREFIXES.some((p) => path.startsWith(p))) return 'forbid';
  if (isRemoteReadPath(path)) {
    if (method !== 'GET') return 'forbid';
    return tokenValid ? 'pass' : 'auth-required';
  }
  return tokenValid ? 'pass' : 'auth-required';
}
