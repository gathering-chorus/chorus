// #3485 — shared-secret auth for POST /api/nudge so the endpoint accepts
// ONLY the MCP server (the single execution path), not anything that sets a
// guessable header. "all of them must point to mcp" (Jeff 2026-06-18).
//
// The secret lives in a 0600 file under ~/.chorus, generated race-safe on
// first use (exclusive create; whoever wins writes, the rest read). Both pulse
// and the mcp-server resolve the SAME path, so no LaunchAgent env wiring is
// required for it to work. CHORUS_PULSE_SECRET (env) overrides for ops/CI.
//
// Fail-open ONLY if the secret is genuinely unresolvable (disk error) — a
// transient FS problem must never break nudge delivery team-wide. When the
// secret resolves (the normal case), the check is strict.

import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';
import { randomBytes, timingSafeEqual } from 'crypto';

function secretPath(): string {
  return (
    process.env.CHORUS_PULSE_SECRET_FILE ||
    path.join(os.homedir(), '.chorus', 'pulse-nudge.secret')
  );
}

let cached: string | null = null;

// Returns the shared secret, or null if it cannot be resolved/created.
export function resolvePulseSecret(): string | null {
  if (process.env.CHORUS_PULSE_SECRET) return process.env.CHORUS_PULSE_SECRET;
  if (cached) return cached;
  const p = secretPath();
  try {
    const existing = fs.readFileSync(p, 'utf8').trim();
    if (existing) {
      cached = existing;
      return cached;
    }
  } catch {
    /* not created yet — fall through to create */
  }
  const secret = randomBytes(32).toString('hex');
  try {
    fs.mkdirSync(path.dirname(p), { recursive: true });
    // 'wx' = exclusive create: throws EEXIST if another process won the race.
    fs.writeFileSync(p, secret, { flag: 'wx', mode: 0o600 });
    cached = secret;
    return cached;
  } catch {
    // Lost the race (EEXIST) or transient error — try to read the winner's.
    try {
      cached = fs.readFileSync(p, 'utf8').trim() || null;
    } catch {
      cached = null;
    }
    return cached;
  }
}

// Constant-time compare; false on length-mismatch or missing input.
export function secretsMatch(provided: string | undefined, expected: string): boolean {
  if (!provided) return false;
  const a = Buffer.from(provided);
  const b = Buffer.from(expected);
  if (a.length !== b.length) return false;
  return timingSafeEqual(a, b);
}

// The single gate predicate. Accepts when:
//   - PULSE_ALLOW_DIRECT_POST=1 (tests / sanctioned migration), OR
//   - the secret is unresolvable (fail-open so delivery never breaks), OR
//   - the provided header matches the resolved secret.
export function callerIsAuthorized(providedHeader: string | undefined): boolean {
  if (process.env.PULSE_ALLOW_DIRECT_POST === '1') return true;
  const expected = resolvePulseSecret();
  if (expected === null) return true; // fail-open: never break nudges on FS error
  return secretsMatch(providedHeader, expected);
}
