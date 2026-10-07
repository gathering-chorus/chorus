// @domain: services
// #4446 — a com.chorus.* node service logs its own start, stop and failure.
//
// The TypeScript/JavaScript twin of platform/services/shared/service_lifecycle.rs:
// same events, same fields, same rule for a kill the service could not log
// (its next start asks launchd how the previous run ended). Plain CommonJS with
// a .d.ts beside it, so compiled TS services and bare node scripts both
// require it. Events go through chorus-log synchronously, so a call from an
// exit handler lands before the process ends.
'use strict';
const { execFileSync } = require('child_process');
const crypto = require('crypto');
const fs = require('fs');
const path = require('path');

/** How the previous run ended, from the top level of `launchctl print`. Null when launchd has none on record. */
function parseLastExit(print) {
  let seen = false;
  const last = { exitCode: null, signal: null };
  for (const line of String(print).split('\n')) {
    if (!line.startsWith('\t') || line.startsWith('\t\t')) continue;
    const i = line.indexOf(' = ');
    if (i < 0) continue;
    const k = line.slice(1, i);
    const v = line.slice(i + 3).trim();
    if (k === 'last exit code') { last.exitCode = parseInt(v.split(':')[0], 10); seen = true; }
    if (k === 'last terminating signal') { last.signal = v; seen = true; }
  }
  return seen ? last : null;
}

/** exit 0, or SIGTERM (bootout, kickstart -k, a deploy restart). */
function clean(last) {
  return last.signal ? last.signal.startsWith('Terminated') : (last.exitCode || 0) === 0;
}

/** The previous run's abnormal end (if any), then this start. */
function startEvents(service, pid, version, previous) {
  const out = [];
  if (previous && !clean(previous)) {
    const f = { service, reason: 'previous run ended abnormally' };
    if (previous.signal) f.signal = previous.signal; else f.exit_code = String(previous.exitCode);
    out.push(['service.failed', f]);
  }
  out.push(['service.started', { service, pid: String(pid), version }]);
  return out;
}

const stopEvent = (service, pid, reason) => ['service.stopped', { service, pid: String(pid), reason }];
const failedEvent = (service, pid, reason, exitCode) =>
  ['service.failed', { service, pid: String(pid), reason, exit_code: String(exitCode) }];

/**
 * launchd sets XPC_SERVICE_NAME for every agent it starts, and everything the
 * agent starts inherits it — so the parent must be launchd (pid 1) too.
 */
function launchdLabel() {
  if (process.ppid !== 1) return null;
  const l = process.env.XPC_SERVICE_NAME || '';
  return l.startsWith('com.chorus.') ? l : null;
}

function previousRun(label) {
  try {
    const uid = process.getuid();
    return parseLastExit(execFileSync('/bin/launchctl', ['print', `gui/${uid}/${label}`], { encoding: 'utf8' }));
  } catch { return null; }
}

/** First 12 hex of the main script's sha256 — names exactly what ran. */
function scriptVersion(file = process.argv[1]) {
  try { return crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex').slice(0, 12); }
  catch { return 'unknown'; }
}

function emit([event, fields]) {
  const home = process.env.CHORUS_HOME || path.join(process.env.HOME || '', 'CascadeProjects/chorus');
  const args = [path.join(home, 'platform/scripts/chorus-log'), event, 'system',
    ...Object.entries(fields).map(([k, v]) => `${k}=${v}`)];
  if (event === 'service.failed') args.push('--level=error');
  try { execFileSync('bash', args, { stdio: 'ignore', timeout: 5000 }); } catch { /* a log line never takes the service down */ }
}

/**
 * One service's lifecycle. `name` is its launchd label, used when the process
 * was not started by launchd (by hand, or under a test).
 */
// #4446 round 2 — the end a service did not log itself: any process.exit(n)
// path (a bad argument, a failed bind) ends as service.failed, a bare exit 0
// as service.stopped. Null when the service already logged its end.
function exitEvent(service, pid, code, ended) {
  if (ended) return null;
  return code === 0 ? stopEvent(service, pid, 'exit 0') : failedEvent(service, pid, `exited ${code}`, code);
}

function serviceLifecycle(name) {
  const service = launchdLabel() || name;
  const pid = process.pid;
  let ended = false;
  return {
    service,
    started(version = scriptVersion()) {
      const fromLaunchd = launchdLabel();
      const previous = fromLaunchd ? previousRun(fromLaunchd) : null;
      for (const e of startEvents(service, pid, version, previous)) emit(e);
      // 'exit' handlers run synchronously; emit is execFileSync, so it lands
      process.once('exit', (code) => {
        const e = exitEvent(service, pid, code, ended);
        if (e) emit(e);
      });
    },
    stopped(reason) { ended = true; emit(stopEvent(service, pid, reason)); },
    failed(reason, exitCode = 1) { ended = true; emit(failedEvent(service, pid, reason, exitCode)); },
    // #4446 reopen — a service that stops itself on a bad argument or config
    // says why, in the log as well as on stderr (python's refuse, same shape)
    refuse(reason, exitCode = 1) {
      console.error(reason);
      ended = true;
      emit(failedEvent(service, pid, reason, exitCode));
      process.exit(exitCode);
    },
  };
}

module.exports = { parseLastExit, startEvents, stopEvent, failedEvent, exitEvent, launchdLabel, previousRun, scriptVersion, serviceLifecycle };
