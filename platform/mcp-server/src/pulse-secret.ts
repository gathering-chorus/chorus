// #3485 — resolve the shared secret the mcp-server presents to pulse's
// POST /api/nudge gate. Mirror of platform/pulse/src/pulse-secret.ts (separate
// package, no shared build target) — both resolve the SAME file path, so the
// secret matches without any LaunchAgent env wiring. CHORUS_PULSE_SECRET (env)
// overrides for ops/CI. Read-only here: pulse owns creation; the mcp-server
// only reads (and tolerates absence — executeNudge still POSTs, and pulse
// fail-opens on its side if the secret is genuinely unresolvable).

import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';

function secretPath(): string {
  return (
    process.env.CHORUS_PULSE_SECRET_FILE ||
    path.join(os.homedir(), '.chorus', 'pulse-nudge.secret')
  );
}

let cached: string | null = null;

export function resolvePulseSecret(): string | null {
  if (process.env.CHORUS_PULSE_SECRET) return process.env.CHORUS_PULSE_SECRET;
  if (cached) return cached;
  try {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- controlled internal path: env override or ~/.chorus/pulse-nudge.secret, never user input
    cached = fs.readFileSync(secretPath(), 'utf8').trim() || null;
  } catch {
    cached = null;
  }
  return cached;
}
