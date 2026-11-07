// @domain: time
// #4885 — the one TypeScript/JavaScript renderer of Boston time. pulse, the api and Clearing each
// carried their own copy (bostonOffsetIso ×3, an approximate isEDT); they import this instead.
// Plain CommonJS with a .d.ts beside it, like lifecycle/; in a browser it is a classic script
// (clearing-v2 serves it at /vendor/chorus-time/boston.js; generated pages inline it). DST comes from the tz database, never a
// month rule. Storage stays an instant; these only render one.
'use strict';

const BOSTON = 'America/New_York';

/** Any other Boston form a screen needs (weekday names, 12-hour clocks): Intl options, Boston zone. */
function bostonFormat(d, opts, locale = 'en-US') {
  return new Intl.DateTimeFormat(locale, { ...opts, timeZone: BOSTON }).format(d);
}

function parts(d) {
  const p = new Intl.DateTimeFormat('en-US', {
    timeZone: BOSTON,
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

/** The Boston calendar day of an instant: 2026-11-04. */
function bostonDay(d = new Date()) { return bostonLocal(d).slice(0, 10); }

/** The Boston hour of an instant, 0-23. */
function bostonHour(d = new Date()) { return Number(parts(d).h); }

/** Boston HH:MM, 24-hour. */
function bostonHHMM(d = new Date()) { return bostonLocal(d).slice(11, 16); }

/** The instant Boston's current day began, as epoch seconds. */
function bostonMidnight(d = new Date()) {
  const x = parts(d);
  return Math.floor(d.getTime() / 1000) - (Number(x.h) * 3600 + Number(x.mi) * 60 + Number(x.s));
}

const api = { bostonOffset, isBostonDaylight, bostonOffsetIso, bostonLocal, bostonMinute,
  bostonDay, bostonHour, bostonHHMM, bostonMidnight, bostonFormat };
// The browser is a runtime too: loaded as a <script>, the same file sets globalThis.chorusBoston.
if (typeof module !== 'undefined' && module.exports) module.exports = api;
else globalThis.chorusBoston = api;
