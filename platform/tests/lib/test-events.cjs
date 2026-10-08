// @test-type: unit — helper for tests and fixtures, not a suite
/**
 * #4454 — a test or fixture says what it is doing, on the run's trace.
 *
 * Writes one `chorus-log --batch` line to $CHORUS_TEST_EVENTS. werk-test sets
 * that path, and after each unit forwards the file's `test.*` lines to the
 * spine, so they reach Loki on the run's trace (CHORUS_TRACE_ID). A test never
 * writes the live spine itself (#3615). Outside werk-test the path is unset
 * and this does nothing.
 */
const fs = require('fs');

function clean(v) {
  return String(v ?? '').replace(/[\t\r\n]/g, ' ');
}

/** testEvent('test.fixture.ready', { fixture: 'own-clearing', port: 3482, ready_ms: 812 }) */
function testEvent(event, fields = {}) {
  const file = process.env.CHORUS_TEST_EVENTS;
  if (!file) return false;
  const kv = { ...fields, at: new Date().toISOString() };
  if (process.env.CHORUS_TRACE_ID) kv.trace = process.env.CHORUS_TRACE_ID;
  if (process.env.CHORUS_CARD_ID) kv.card = process.env.CHORUS_CARD_ID;
  const line = [event, 'tests', ...Object.entries(kv).map(([k, v]) => `${k}=${clean(v)}`)].join('\t');
  try {
    fs.appendFileSync(file, line + '\n');
    return true;
  } catch {
    return false;
  }
}

module.exports = { testEvent };
