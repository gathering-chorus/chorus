// @domain: services
// @test-type: unit
// #4446 — the node twin of shared/service_lifecycle.rs: same events, same rule
// for a kill the service could not log. Fixtures are real `launchctl print`.
import { parseLastExit, startEvents, stopEvent, failedEvent, exitEvent, launchdLabel } from '../lifecycle/service-lifecycle';

const KILLED = 'gui/501/com.chorus.api = {\n\truns = 15\n\tpid = 52021\n\tlast terminating signal = Killed: 9\n\tendpoints = {\n\t\tlast exit code = 0\n\t}\n}';
const TERMINATED = 'gui/501/com.chorus.api = {\n\truns = 15\n\tlast terminating signal = Terminated: 15\n}';
const EX_CONFIG = 'gui/501/com.chorus.nudge-health = {\n\truns = 3325\n\tlast exit code = 78: EX_CONFIG\n}';
const FIRST_RUN = 'gui/501/com.chorus.api = {\n\truns = 1\n\tpid = 52021\n}';

const names = (ev: [string, Record<string, string>][]) => ev.map(([e]) => e);

describe('service lifecycle (#4446)', () => {
  it('reads how the previous run ended, ignoring nested blocks', () => {
    expect(parseLastExit(KILLED)).toEqual({ exitCode: null, signal: 'Killed: 9' });
    expect(parseLastExit(EX_CONFIG)?.exitCode).toBe(78);
    expect(parseLastExit(FIRST_RUN)).toBeNull();
  });

  it('a start after a kill reports the kill, then the start', () => {
    const ev = startEvents('com.chorus.api', 4242, 'abc123def456', parseLastExit(KILLED));
    expect(names(ev)).toEqual(['service.failed', 'service.started']);
    expect(ev[0][1].signal).toBe('Killed: 9');
    expect(ev[1][1]).toEqual({ service: 'com.chorus.api', pid: '4242', version: 'abc123def456' });
  });

  it('a start after an error exit carries the exit code', () => {
    expect(startEvents('x', 1, 'v', parseLastExit(EX_CONFIG))[0][1].exit_code).toBe('78');
  });

  it('negative proof: a clean previous end (SIGTERM, or none) is not a failure', () => {
    expect(names(startEvents('x', 1, 'v', parseLastExit(TERMINATED)))).toEqual(['service.started']);
    expect(names(startEvents('x', 1, 'v', null))).toEqual(['service.started']);
    // a first run: launchd prints "(never exited)" — not a failure
    const never = parseLastExit('x = {\n\tlast exit code = (never exited)\n}');
    expect(names(startEvents('x', 1, 'v', never))).toEqual(['service.started']);
  });

  it('stop and failure carry their reason', () => {
    expect(stopEvent('com.chorus.api', 7, 'SIGTERM')).toEqual(['service.stopped', { service: 'com.chorus.api', pid: '7', reason: 'SIGTERM' }]);
    expect(failedEvent('com.chorus.api', 7, 'uncaughtException: boom', 1)[1].exit_code).toBe('1');
  });

  it('negative proof: an inherited label is not launchd starting us', () => {
    // Everything a service starts inherits XPC_SERVICE_NAME; only a process whose
    // parent is launchd is that service. jest's parent is not. (The positive half
    // runs under real launchd jobs in 4446-service-lifecycle.bats.)
    const saved = process.env.XPC_SERVICE_NAME;
    process.env.XPC_SERVICE_NAME = 'com.chorus.pulse';
    expect(launchdLabel()).toBeNull();
    if (saved === undefined) delete process.env.XPC_SERVICE_NAME; else process.env.XPC_SERVICE_NAME = saved;
  });
});

// #4446 round 2 — error handling: an exit the service did not log itself.
describe('an exit the service did not log', () => {
  it('a non-zero exit is service.failed with its code; exit 0 is a stop', () => {
    expect(exitEvent('s', 1, 3, false)).toEqual(failedEvent('s', 1, 'exited 3', 3));
    expect(exitEvent('s', 1, 0, false)).toEqual(stopEvent('s', 1, 'exit 0'));
  });

  it('NEGATIVE PROOF: an end the service already logged is not logged twice', () => {
    expect(exitEvent('s', 1, 1, true)).toBeNull();
  });

});
