// @domain: time
// #4885 — the one TypeScript/JavaScript renderer of Boston time. pulse, the api and Clearing each
// carried their own copy (bostonOffsetIso ×3, an approximate isEDT); they import this instead.
// Plain CommonJS with a .d.ts beside it, like lifecycle/. DST comes from the tz database, never a
// month rule. Storage stays an instant; these only render one.
'use strict';

function parts(d) {
  const p = new Intl.DateTimeFormat('en-US', {
    timeZone: 'America/New_York',
    year: 'numeric', month: '2-digit', day: '2-digit',
    hour: '2-digit', minute: '2-digit', second: '2-digit',
    hour12: false, timeZoneName: 'longOffset',
  }).formatToParts(d);
  const get = (t) => (p.find((x) => x.type === t) || {}).value || '';
  return {
    y: get('year'), mo: get('month'), da: get('day'),
    h: get('hour') === '24' ? '00' : get('hour'), mi: get('minute'), s: get('second'),
    off: (get('timeZoneName') || 'GMT+00:00').replace('GMT', '') || '+00:00',
  };
}

/** Boston's UTC offset at this instant, "-04:00" or "-05:00". */
function bostonOffset(d = new Date()) { return parts(d).off; }

/** True when Boston keeps daylight time at this instant (from the tz database). */
function isBostonDaylight(d = new Date()) { return bostonOffset(d) === '-04:00'; }

/** Offset-ISO Boston, the spine's one clock: 2026-11-04T04:26:30.123-05:00. */
function bostonOffsetIso(d = new Date()) {
  const x = parts(d);
  const ms = String(d.getMilliseconds()).padStart(3, '0');
  return `${x.y}-${x.mo}-${x.da}T${x.h}:${x.mi}:${x.s}.${ms}${x.off}`;
}

/** Boston wall time to the second, no zone: 2026-11-04 04:26:30 (a display label, never stored). */
function bostonLocal(d = new Date()) {
  const x = parts(d);
  return `${x.y}-${x.mo}-${x.da} ${x.h}:${x.mi}:${x.s}`;
}

/** The minute a message header shows: 2026-11-04 04:26. */
function bostonMinute(d = new Date()) { return bostonLocal(d).slice(0, 16); }

module.exports = { bostonOffset, isBostonDaylight, bostonOffsetIso, bostonLocal, bostonMinute };
