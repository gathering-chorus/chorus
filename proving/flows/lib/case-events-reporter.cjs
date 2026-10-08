// @test-type: unit — playwright reporter, not a suite
/**
 * #4454 — each browser-flow case says when it starts and how it ended, on the
 * run's trace: a spec that hangs shows as started with no end, and a red
 * carries its error. Events go through test-events.cjs (werk-test forwards).
 */
const path = require('path');
const { testEvent } = require('../../../platform/tests/lib/test-events.cjs');

const ROOT = path.resolve(__dirname, '..', '..', '..');

function where(test) {
  return { file: path.relative(ROOT, test.location.file), case: test.titlePath().slice(2).join(' > ') || test.title };
}

class CaseEventsReporter {
  onTestBegin(test) {
    testEvent('test.case.started', where(test));
  }

  onTestEnd(test, result) {
    const status = result.status === 'passed' ? 'passed' : result.status === 'skipped' ? 'skipped' : 'failed';
    const fields = { ...where(test), unit: 'playwright', elapsed_ms: result.duration };
    if (status === 'failed') {
      const err = (result.error && (result.error.message || result.error.value)) || result.status;
      fields.failure_kind = result.status;
      fields.reason = String(err).split('\n').slice(0, 3).join(' ').slice(0, 500);
      fields.level = 'error';
    }
    fields.message = status === 'failed'
      ? `${fields.case} failed in ${fields.file}: ${fields.reason}`
      : `${fields.case} ${status} in ${fields.file}`;
    testEvent(`test.case.${status}`, fields);
  }
}

module.exports = CaseEventsReporter;
