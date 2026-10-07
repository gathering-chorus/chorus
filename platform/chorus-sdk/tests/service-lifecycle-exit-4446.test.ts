// @domain: services
// @test-type: integration
// #4446 round 2 — error handling, end to end: a real node process that starts
// the lifecycle and then calls process.exit(3) leaves service.failed in the log
// (written by chorus-log into a temp CHORUS_LOG_FILE, never the live spine).
import { execFileSync } from 'child_process';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';

describe('an exit the service did not log, in a real process', () => {
  it('a real service that calls process.exit(3) after starting leaves service.failed in the log', () => {
    const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'sl4446-'));
    const log = path.join(tmp, 'chorus.log');
    const root = path.resolve(process.cwd(), '..', '..');
    const lib = path.join(root, 'platform/chorus-sdk/lifecycle/service-lifecycle.js');
    const script = `const l = require(${JSON.stringify(lib)}).serviceLifecycle('com.chorus.fixture-exit'); l.started('v'); process.exit(3);`;
    let status = 0;
    try {
      execFileSync(process.execPath, ['-e', script], {
        env: { ...process.env, CHORUS_HOME: root, CHORUS_LOG_FILE: log, CHORUS_CONTEXT: 'test', XPC_SERVICE_NAME: '' },
      });
    } catch (e) {
      status = (e as { status: number }).status;
    }
    expect(status).toBe(3);
    const lines = fs.readFileSync(log, 'utf8');
    expect(lines).toMatch(/"event":"service.started"/);
    expect(lines).toMatch(/"event":"service.failed".*"exit_code":"?3/);
    fs.rmSync(tmp, { recursive: true, force: true });
  });
});
