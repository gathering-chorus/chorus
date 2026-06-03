// chorus-paths.ts — single app-side source for the chorus checkout path (#3197).
//
// The same value was re-derived in 9 places as `process.env.CHORUS_ROOT ||
// <each-site's-own-guess>`, and the guesses had already drifted: three
// different fallbacks, one of them wrong (server.ts read
// '/Users/.../CascadeProjects' — missing the /chorus segment). That is the
// "hardcode the same thing in N places" failure: N copies, N chances to drift.
//
// One definition lives here. Every site imports CHORUS_ROOT/CHORUS_HOME from
// this module instead of reading process.env directly. The env var itself is
// set by the one shell source, platform/scripts/chorus-env-setup.sh; this is
// its app-side mirror with a single homedir-derived fallback (no username
// literal) for the rare process that starts without the env populated.
import os from 'os';
import path from 'path';

export const CHORUS_ROOT =
  process.env.CHORUS_ROOT || path.join(os.homedir(), 'CascadeProjects', 'chorus');

// CHORUS_HOME is the canonical checkout; equals CHORUS_ROOT outside a werk.
export const CHORUS_HOME = process.env.CHORUS_HOME || CHORUS_ROOT;
