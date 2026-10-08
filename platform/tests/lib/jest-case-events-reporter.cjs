// @test-type: unit — jest reporter, not a suite
/**
 * #4454 — each jest case says when it starts, on the run's trace, so a case
 * that hangs shows as started with no end. werk-test logs how each case ended
 * (test.case.passed / failed, with its time and reason) from jest's --json.
 * Events go through test-events.cjs; werk-test forwards them.
 */
const path = require('path');
const { testEvent } = require('./test-events.cjs');

const ROOT = path.resolve(__dirname, '..', '..', '..');

class JestCaseEventsReporter {
  onTestCaseStart(test, info) {
    const file = path.relative(ROOT, test.path);
    const name = info.fullName || [...(info.ancestorTitles || []), info.title].join(' ');
    testEvent('test.case.started', { file, case: name, unit: 'jest', message: `${name} started in ${file}` });
  }
}

module.exports = JestCaseEventsReporter;
