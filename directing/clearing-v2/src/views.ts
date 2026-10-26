import type { BoardColumn } from './sources/board';
// @domain: messages

import { TrustStripSource, RolesData, TodayData, RoomsData, ActorRow } from './data-service';
import { DAY_COLUMNS, DOUBLE_COLUMNS, type DayCell, type DayRow, type SyncDay } from './sources/readings';
import type { Streams } from './sources/streams';
import { uploadNames } from './uploads';
import fs from 'fs';
import path from 'path';

/** #4635 — Cloudflare hands phones these files with a 4-hour cache (measured: max-age=14400),
 *  so Jeff's iPhone kept the old page script and stylesheet: the photo button did nothing and
 *  the icon words showed. Each URL carries the files' newest change time, so a new version is
 *  a new URL and is fetched at once. */
export const ASSET_VERSION = ((): string => {
  const dir = path.join(__dirname, '../public');
  try {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- fixed names under this package's own public dir
    return String(Math.floor(Math.max(...['app.js', 'style.css', 'room-key.js'].map((f) => fs.statSync(path.join(dir, f)).mtimeMs))));
  } catch {
    return String(Date.now());
  }
})();

function renderTrustChip(s: TrustStripSource): string {
  if (s.status === 'failed' || s.status === 'unmeasured') {
    return `<span class="chip strip-source-fail"><i class="fail"></i><span class="fail-source-name">${s.source}</span> ${s.status} (${s.reason || 'no reason sent'})</span>`;
  }
  if (s.lagMinutes && s.lagMinutes >= 30) {
    return `<span class="chip strip-lag-neutral"><i class="lag"></i>${s.source} · ${s.asOf} (${s.lagMinutes}m)</span>`;
  }
  return `<span class="chip"><i></i>${s.source} · ${s.asOf}</span>`;
}

function countBadge(n: number | undefined): string {
  return n && n > 0 ? `<span class="n">${n}</span>` : '';
}

function renderNav(activeNav: string, counts?: { decisions?: number; alerts?: number }): string {
  const on = (nav: string): string => (activeNav === nav ? 'on' : '');
  return `<nav class="top">
    <span class="brand">Clearing</span>
    <a href="/today" class="${on('today')}">Today</a>
    <a href="/decisions" class="${on('decisions')}">Decisions${countBadge(counts?.decisions)}</a>
    <a href="/rooms/team" class="${on('rooms')}">Rooms</a>
    <a href="/board" class="${on('board')}">Board</a>
    <a href="/roles" class="${on('roles')}">Teammates</a>
    <a href="/streams" class="${on('streams')}">Streams</a>
    <a href="/calendar" class="${on('calendar')}">Calendar</a>
    <a href="/plans" class="${on('plans')}">Plans</a>
    <a href="/alerts" class="${on('alerts')}">Alerts${countBadge(counts?.alerts)}</a>
  </nav>`;
}

/** #4575 — Jeff 07:01: "i dont see a way to send now in board". Every page carries the
 *  #team composer (Rooms has its own, per room); no @ goes to Wren, as in the room. */
const TEAM_COMPOSER = `<footer class="composer-bar page-composer">
    <form id="composer-form">
      <textarea id="composer-input" rows="1" placeholder="Message #team..." autocomplete="off"></textarea>
      <label for="composer-photo" class="composer-photo" title="Attach a photo" aria-label="Attach a photo"><input type="file" id="composer-photo" accept="image/*" multiple class="sr-only">📷</label>
      <button type="button" class="composer-mic" id="composer-mic" title="Record voice" aria-label="Record voice" aria-pressed="false">🎤</button>
      <button type="submit" id="composer-send">Send</button>
    </form>
    <div id="send-error" class="send-error" role="alert"></div>
  </footer>`;

export function renderPageShell(
  title: string,
  activeNav: string,
  bodyContent: string,
  sources: TrustStripSource[] = [],
  counts?: { decisions?: number; alerts?: number }
): string {
  const chipsHtml = sources.map(renderTrustChip).join('');

  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0, viewport-fit=cover">
  <title>${title} · The Clearing v2</title>
  <link rel="stylesheet" href="/style.css?v=${ASSET_VERSION}">
  <script type="importmap">
  {
    "imports": {
      "@noble/hashes/utils": "/vendor/@noble/hashes/utils.js",
      "@noble/hashes/": "/vendor/@noble/hashes/",
      "@noble/curves/": "/vendor/@noble/curves/"
    }
  }
  </script>
  <script>
    window.CHORUS_WEBID = "https://id.lightlifeurbangardens.com/jeff/profile/card#me";
  </script>
  <script type="module" src="/room-key.js?v=${ASSET_VERSION}"></script>
</head>
<body>
  <header class="app-header">
    <div class="header-left">
      <h1>The Clearing <span class="badge-v2">v2</span></h1>
      <span class="relay-status" id="relay-indicator">Connecting...</span>
    </div>
    <div class="strip trust-strip" id="trust-dots">
      ${chipsHtml}
    </div>
  </header>
  ${renderNav(activeNav, counts)}
  ${bodyContent}
  ${activeNav === 'rooms' ? '' : TEAM_COMPOSER}
  <script src="/app.js?v=${ASSET_VERSION}"></script>
</body>
</html>`;
}


/** Dot colour for pulse's state word (presentation only; the word itself is shown verbatim). */
function stateDot(state: string): string {
  switch (state) {
    case 'building':
    case 'pairing':
    case 'reviewing':
    case 'in-ceremony':
      return 'dot d-ok';
    case 'waiting':
      return 'dot d-warn';
    case 'available':
      return 'dot d-info';
    case 'away':
    case 'off':
      return 'dot d-mut state-faint';
    case 'unmeasured':
      return 'dot d-bad';
    default:
      return 'dot d-mut';
  }
}

function renderStateBadge(a: ActorRow): string {
  const faint = a.state === 'away' || a.state === 'off' ? ' style="background:transparent;border:1.5px solid var(--faint)"' : '';
  return `<span class="st role-state-badge state-${a.state}"><span class="${stateDot(a.state)}"${faint}></span><span class="state-word">${a.state}</span></span>`;
}

function renderActorDetail(a: ActorRow): string {
  return [
    a.pair ? `<div class="meta pair">${a.pair}</div>` : '',
    a.waiting ? `<div class="meta waiting-reason">${a.waiting}</div>` : '',
    a.unmeasured ? `<div class="meta fail actor-unmeasured">unmeasured: ${a.unmeasured}</div>` : '',
  ].join('');
}

export function renderCardRefs(
  cards?: Array<{ id: number; title: string; step?: string }>,
  card?: ActorRow['card'],
  pair?: string
): string {
  const list = cards && cards.length > 0 ? cards : (card ? [card] : []);
  if (list.length === 0) return '<span class="meta">–</span>';
  return list
    .map((c) => {
      const pairText = pair ? ` <span class="meta pair-tag">(${escapeHtml(pair)})</span>` : '';
      return `<a class="cid" href="/board?card=${c.id}">#${c.id}</a>${c.title ? ` <span class="meta">${escapeHtml(c.title)}</span>` : ''}${pairText}`;
    })
    .join('<br>');
}

export function renderActivity(a: ActorRow): string {
  if (a.activity === 'idle') return 'idle';
  if (a.activity === 'unmeasured') return 'unmeasured';
  const list = a.cards && a.cards.length > 0 ? a.cards : (a.card ? [a.card] : []);
  if (list.length > 0) {
    return list.map((c) => escapeHtml(c.step || 'coding')).join('<br>');
  }
  return a.activity || '–';
}

export function roleAndonLight(actor?: ActorRow): { status: 'working' | 'waiting' | 'idle' | 'stalled'; cssClass: string } {
  if (!actor) return { status: 'idle', cssClass: 'andon-idle' };
  if (actor.state === 'waiting') {
    return { status: 'waiting', cssClass: 'andon-waiting' };
  }
  if (actor.state === 'stalled' || actor.state === 'unmeasured') {
    return { status: 'stalled', cssClass: 'andon-stalled' };
  }
  if (actor.activity === 'acting') {
    return { status: 'working', cssClass: 'andon-working' };
  }
  if (actor.activity === 'idle') {
    return { status: 'idle', cssClass: 'andon-idle' };
  }
  if (actor.state === 'building' || actor.state === 'pairing' || actor.state === 'reviewing') {
    return { status: 'working', cssClass: 'andon-working' };
  }
  return { status: 'idle', cssClass: 'andon-idle' };
}

export function ownerPresence(ownerName?: string, rolesData?: RolesData): { status: 'working' | 'awake' | 'quiet'; text: string; dotClass: string } {
  if (!ownerName) return { status: 'quiet', text: 'quiet', dotClass: 'd-mut' };
  const slug = ownerName.toLowerCase().replace(/\s+/g, '-').replace(/^principal-/, '');
  const actor = rolesData?.actors.find((a) => a.id.toLowerCase() === slug || a.name.toLowerCase() === ownerName.toLowerCase());
  if (!actor) return { status: 'quiet', text: 'quiet', dotClass: 'd-mut' };
  if (actor.activity === 'acting') {
    return { status: 'working', text: 'working', dotClass: 'd-ok andon-working' };
  }
  if (actor.state === 'available' || actor.sessions.some((s) => s.includes('live'))) {
    return { status: 'awake', text: 'awake', dotClass: 'd-info' };
  }
  return { status: 'quiet', text: 'quiet', dotClass: 'd-mut' };
}

function renderUnmeasured(cls: string, text: string | undefined): string {
  return text ? `<div class="meta fail ${cls}">unmeasured: ${text}</div>` : '';
}

/** Tag colour for pulse's card status (presentation only; the status is shown verbatim). */
function statusTag(status: string): string {
  const s = status.toLowerCase();
  if (s === 'done') return 't-ok';
  if (s === 'blocked') return 't-bad';
  return 't-mut';
}

function renderWorkingNow(data: TodayData): string {
  const rows = data.workingRoles.map(w => `
            <div class="li">
              <span class="av a-${w.id}">${w.name.charAt(0)}</span>
              <div class="grow">${w.name} · ${renderStateBadge(w)}
                <div class="meta">${renderCardRefs(w.cards, w.card)}</div>
                ${renderActorDetail(w)}
              </div>
            </div>
          `).join('');
  const empty = !data.workingUnmeasured && data.workingRoles.length === 0
    ? '<div class="empty-state meta" style="padding:6px 0;color:var(--faint)">pulse actor-state listed no actors</div>'
    : '';
  return `${renderUnmeasured('working-unmeasured', data.workingUnmeasured)}${rows}${empty}`;
}

function renderChangedOvernight(data: TodayData): string {
  const rows = data.changedCards.slice(0, 4).map(c => `
            <div class="li">
              <span class="tag ${statusTag(c.status)}">${c.status}</span>
              <div class="grow"><a class="cid" href="/board?card=${c.id}">#${c.id}</a> ${c.title}<div class="meta">${c.owner}${c.time ? ` · ${c.time}` : ''}</div></div>
            </div>
          `).join('');
  const empty = !data.changedUnmeasured && data.changedCards.length === 0
    ? '<div class="empty-state meta" style="padding:4px 0;color:var(--faint)">pulse changes-overnight listed no cards</div>'
    : '';
  return `${renderUnmeasured('changed-unmeasured', data.changedUnmeasured)}<div class="list">${rows}${empty}</div>`;
}

export function renderTodayPage(data: TodayData, sources: TrustStripSource[]): string {
  const content = `
  <div class="page today-page">
    <div class="ph"><h2>${data.timeStr}</h2><span class="ex">live data</span></div>
    <div class="grid2">
      <!-- Decisions owed (requires Part 2 Decision class) -->
      <div class="card decisions-owed" style="grid-column:1/-1">
        <div class="sec-t">Decisions owed · ${data.decisionsCount > 0 ? data.decisionsCount : 'no source (part 2)'}</div>
        ${data.decisionsCount > 0 ? `
          <div class="grp">
            <div class="grp-h">
              <div><h3>Overnight triage</h3><div class="meta mono">${data.decisions.length} decisions</div></div>
              <div class="row"><button class="btn">Open</button><button class="btn pri approve-all">Approve all ${data.decisions.length}</button></div>
            </div>
            <div class="list">
              ${data.decisions.map(d => `<div class="li"><span class="dot d-warn"></span><div class="grow">${d.title}<div class="meta">evidence: ${d.evidence.join(' ')}</div></div></div>`).join('')}
            </div>
          </div>
        ` : `
          <div class="empty-state mono meta" style="padding:4px 0;color:var(--faint)">no source (part 2)</div>
        `}
      </div>

      <!-- Changed overnight: the cards pulse /api/changes-overnight returns -->
      <div class="card changed-overnight">
        <div class="sec-t">Changed overnight${data.watermark ? ` · since ${data.watermark}` : ''}${data.changedUnmeasured ? ' · unmeasured' : ''}</div>
        ${renderChangedOvernight(data)}
        ${data.foldedCount > 0 ? `<div class="meta moves-folded" style="margin-top:6px">+ ${data.foldedCount} more moves · <a class="cid">show</a></div>` : ''}
      </div>

      <!-- Alerts & Working now -->
      <div class="card alerts-section">
        <div class="sec-t">Alerts · ${data.alertsUnmeasured ? 'unmeasured' : `${data.alerts.length} firing`}</div>
        ${renderUnmeasured('alerts-unmeasured', data.alertsUnmeasured)}
        <div class="list">
          ${data.alerts.length > 0 ? data.alerts.map(a => `
            <div class="li">
              <span class="tag ${a.status === 'firing' ? 't-bad' : 't-warn'}">${a.status}</span>
              <div class="grow">${a.title}<div class="meta">${a.meta}${a.resolvingCard > 0 ? ` · <a class="cid alert-resolving-card" href="/board?card=${a.resolvingCard}">#${a.resolvingCard}</a>` : ''}</div></div>
            </div>
          `).join('') : data.alertsUnmeasured ? '' : `
            <div class="empty-state mono meta" style="padding:4px 0;color:var(--faint)">0 firing</div>
          `}
        </div>
        <div class="sec-t working-now" style="margin-top:14px">Working now</div>
        <div class="list">
          ${renderWorkingNow(data)}
        </div>
      </div>
    </div>
  </div>`;

  return renderPageShell('Today', 'today', content, sources, { decisions: data.decisionsCount, alerts: data.alerts.length });
}

export function renderRolesPage(data: RolesData, sources: TrustStripSource[]): string {
  const table = data.actors.length === 0 ? '' : `
        <table class="roles-table">
          <thead>
            <tr><th>Teammate</th><th>State</th><th>Activity</th><th>Card</th><th>Sessions</th></tr>
          </thead>
          <tbody>
            ${data.actors.map(a => `
              <tr class="${a.id === 'jeff' ? 'you' : ''}">
                <td><span class="row"><span class="av a-${a.id}">${a.name.charAt(0)}</span><b>${a.name}</b></span></td>
                <td>${renderStateBadge(a)}${renderActorDetail(a)}</td>
                <td class="meta">${renderActivity(a)}</td>
                <td>${renderCardRefs(a.cards, a.card, a.pair)}</td>
                <td class="mono meta role-sessions" title="${a.sessions.join(' · ')}">${a.sessions.length > 0 ? a.sessions[0] + (a.sessions.length > 1 ? ` <span class="faint">+${a.sessions.length - 1}</span>` : '') : '–'}</td>
              </tr>
            `).join('')}
          </tbody>
        </table>`;
  const empty = !data.unmeasured && data.actors.length === 0
    ? '<div class="empty-state roles-empty" style="padding:20px;color:var(--faint)">pulse actor-state listed no actors</div>'
    : '';
  const content = `
  <div class="page roles-page">
    <div class="ph"><h2>Teammates</h2><span class="ex">pulse actor-state</span></div>
    <div class="tw">
      ${renderUnmeasured('roles-unmeasured', data.unmeasured)}
      ${empty}
      ${table}
    </div>
  </div>`;

  return renderPageShell('Teammates', 'roles', content, sources);
}

function formatInlineHtml(text: string): string {
  let escaped = escapeHtml(text);

  // Preserve inline code first so its contents are not bolded or linked
  const inlineCodes: string[] = [];
  escaped = escaped.replace(/`([^`\n]+)`/g, (_match, code) => {
    inlineCodes.push(`<code>${code}</code>`);
    return `\x00CODE_${inlineCodes.length - 1}\x00`;
  });

  // Bold **...**
  escaped = escaped.replace(/\*\*([^*\n]+)\*\*/g, '<strong>$1</strong>');

  // URLs (http/https): clickable, open in new tab (AC2)
  escaped = escaped.replace(/(https?:\/\/[^\s<>"']+)/g, (_match, url) => {
    const m = url.match(/^(https?:\/\/[^\s<>"']+?)([.,;!?)]*)$/);
    const cleanUrl = m ? m[1] : url;
    const punct = m ? m[2] : '';
    return `<a href="${cleanUrl}" target="_blank" rel="noopener noreferrer">${cleanUrl}</a>${punct}`;
  });

  // #card refs
  escaped = escaped.replace(/#(\d+)/g, '<a class="cid card-ref" href="#" onclick="openCardDrawer(\'$1\');return false;">#$1</a>');

  // Restore inline code
  inlineCodes.forEach((code, i) => {
    escaped = escaped.replace(`\x00CODE_${i}\x00`, code);
  });

  return escaped;
}

export function formatRoomText(raw: string): string {
  // 1. Extract code fences first so their contents are preserved literally (AC4)
  const fences: string[] = [];
  const textWithoutFences = raw.replace(/(```[a-zA-Z0-9_-]*\n?[\s\S]*?```)/g, (_match, fence) => {
    const m = fence.match(/^```([a-zA-Z0-9_-]*)\n?([\s\S]*?)```$/);
    const lang = m ? m[1] : '';
    const code = m ? m[2].replace(/\n$/, '') : fence.slice(3, -3);
    const langCls = lang ? ` class="language-${escapeHtml(lang)}"` : '';
    fences.push(`<pre><code${langCls}>${escapeHtml(code)}</code></pre>`);
    return `\x00FENCE_${fences.length - 1}\x00`;
  });

  // 2. Format lines and list blocks
  const lines = textWithoutFences.split('\n');
  const output: string[] = [];
  let inUl = false;
  let inOl = false;

  for (const [i, line] of lines.entries()) {
    const bulletMatch = line.match(/^[-*]\s+(.*)$/);
    const numberedMatch = line.match(/^(\d+)\.\s+(.*)$/);

    if (bulletMatch) {
      if (inOl) { output.push('</ol>'); inOl = false; }
      if (!inUl) { output.push('<ul>'); inUl = true; }
      output.push(`<li>${formatInlineHtml(bulletMatch[1])}</li>`);
    } else if (numberedMatch) {
      if (inUl) { output.push('</ul>'); inUl = false; }
      if (!inOl) { output.push('<ol>'); inOl = true; }
      output.push(`<li>${formatInlineHtml(numberedMatch[2])}</li>`);
    } else {
      if (inUl) { output.push('</ul>'); inUl = false; }
      if (inOl) { output.push('</ol>'); inOl = false; }

      if (line.includes('\x00FENCE_')) {
        output.push(line);
      } else {
        if (i > 0 && output.length > 0 && !output[output.length - 1].endsWith('</ul>') && !output[output.length - 1].endsWith('</ol>')) {
          output.push('<br>');
        }
        if (line.length > 0) {
          output.push(formatInlineHtml(line));
        }
      }
    }
  }
  if (inUl) output.push('</ul>');
  if (inOl) output.push('</ol>');

  let result = output.join('');

  // Restore code fences
  fences.forEach((fence, i) => {
    result = result.replace(`\x00FENCE_${i}\x00`, fence);
  });

  return result;
}

/** #4635 — a message that names an uploaded photo shows it, tap to open full size. */
export function roomPhotos(text: string): string {
  const names = uploadNames(text);
  if (names.length === 0) return '';
  return `<div class="msg-photos">${names.map((n) => `<a href="/uploads/${n}" target="_blank" rel="noopener"><img src="/uploads/${n}" alt="photo ${n}" loading="lazy"></a>`).join('')}</div>`;
}

/** #4646 — a short fingerprint of a message's words; app.js computes the same one, so
 *  the page can spot a second copy without carrying the whole text in an attribute. */
export function bodyHash(text: string): string {
  let h = 5381;
  for (let i = 0; i < text.length; i++) h = ((h << 5) + h + text.charCodeAt(i)) | 0;
  return (h >>> 0).toString(36);
}

function formatAuthorName(role: string): string {
  if (role === 'abby-normal') return 'Abby Normal';
  if (role === 'borg-queen') return 'Borg Queen';
  return role;
}

export function renderRoomMessage(m: RoomsData['messages'][number]): string {
  const linkifiedText = formatRoomText(m.text);
  const isThinking = m.type === 'thinking';
  const thinkingClass = isThinking ? ' msg-thinking' : '';
  const thinkingBadge = isThinking ? '<span class="badge-thinking">thinking</span>' : '';
  return `
    <div class="msg${thinkingClass}" id="msg-${m.id}" data-from="${escapeHtml(m.from)}" data-body="${bodyHash(String(m.text || '').trim())}" data-ts="${escapeHtml(String(m.ts || ''))}" data-msg-type="${m.type || 'message'}"${m.kind ? ` data-kind="${m.kind}"` : ''}>
      <span class="av a-${m.from}">${m.from.charAt(0).toUpperCase()}</span>
      <div class="body">
        <span class="nm who-${m.from}">${formatAuthorName(m.from)}</span>
        <span class="tm">${m.time}</span>
        ${m.tick ? '<span class="tick delivery-tick">✓ relay</span>' : ''}
        ${thinkingBadge}
        <div>${linkifiedText}</div>
        ${roomPhotos(m.text)}
        ${m.threadCount ? `
          <div class="thread-container" style="margin-top:6px">
            <span class="thr">${m.threadCount} working replies${m.threadLast ? ` · last ${m.threadLast}` : ''}</span>
          </div>
        ` : ''}
      </div>
    </div>
  `;
}

// #4592 / #4618 — the "team nudges" toggle: hides or shows nudges between roles and
// system alerts (anything with data-kind). Off by default to keep the room focused
// on primary signals (Jeff 10-18 13:43), remembered per device. Replies carry no kind.
const TEAM_NUDGES_KEY = 'clearing-v2.teamNudges';
const TEAM_NUDGES_TOGGLE = `  <style>body.hide-team-nudges [data-kind] { display: none; }</style>
  <script>
    (function () {
      var box = document.getElementById('team-nudges');
      var on = false;
      try { on = localStorage.getItem('${TEAM_NUDGES_KEY}') === 'on'; } catch (e) {}
      function apply(v) { document.body.classList.toggle('hide-team-nudges', !v); if (box) box.checked = v; }
      apply(on);
      if (box) box.addEventListener('change', function () {
        try { localStorage.setItem('${TEAM_NUDGES_KEY}', box.checked ? 'on' : 'off'); } catch (e) {}
        apply(box.checked);
      });
    })();
  </script>`;

const ROOM_CARD_DRAWER = `    <aside class="card-drawer" id="card-drawer" style="display:none">
      <div class="row" style="justify-content:space-between;align-items:center">
        <a class="cid" id="drawer-card-link" href="#" target="_blank"></a>
        <button class="btn ghost drawer-close-btn" onclick="closeCardDrawer()" style="font-size:16px;padding:4px 8px;cursor:pointer">✕</button>
      </div>
      <h3 id="drawer-card-title" style="margin-top:10px">Loading card...</h3>
      <div class="meta" id="drawer-card-meta" style="margin-top:6px;line-height:1.5"></div>
      <div class="meta" id="drawer-card-domains" style="margin-top:4px;font-size:11.5px;color:var(--muted);display:none"></div>
      <div class="meta" id="drawer-card-dates" style="margin-top:4px;font-size:11px;color:var(--faint);display:none"></div>

      <div id="drawer-card-desc-container" style="display:none;margin-top:14px;border-top:1px solid var(--line2);padding-top:10px">
        <div class="sec-t" style="margin-bottom:6px">Description</div>
        <div class="card-desc-body" id="drawer-card-desc" style="font-size:13px;line-height:1.45;white-space:pre-wrap;color:var(--ink-secondary, inherit)"></div>
      </div>

      <div id="drawer-card-ac-container" style="display:none;margin-top:14px;border-top:1px solid var(--line2);padding-top:10px">
        <div class="sec-t" style="margin-bottom:6px">Acceptance Criteria</div>
        <div class="card-ac-list" id="drawer-card-ac"></div>
      </div>

      <div id="drawer-card-comments-container" style="display:none;margin-top:14px;border-top:1px solid var(--line2);padding-top:10px">
        <div class="sec-t" style="margin-bottom:6px">Recent Comments</div>
        <div class="card-comments-list" id="drawer-card-comments"></div>
      </div>

      <div id="drawer-card-cws-container" style="margin-top:14px;border-top:1px solid var(--line2);padding-top:10px">
        <div class="row" style="justify-content:space-between;align-items:center;margin-bottom:6px">
          <div class="sec-t">Pipeline State <span class="meta mono">/cws</span></div>
          <div id="drawer-cws-indicator"></div>
        </div>
        <div id="drawer-card-cws-motion" class="cws-motion-bar" style="display:none;margin-bottom:8px"></div>
        <pre class="cws-output" id="drawer-card-cws" style="font-size:11px;font-family:var(--font-mono, monospace);white-space:pre-wrap;background:var(--bg-primary, #111);padding:10px;border-radius:6px;overflow-x:auto;max-height:360px">Loading pipeline state...</pre>
      </div>
    </aside>`;

const ROOM_DRAWER_SCRIPT = `  <script>
    var currentCardDrawerId = null;
    var currentPipelineTimer = null;

    function drawerEscape(s) {
      if (!s) return '';
      return String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
    }

    async function openCardDrawer(id) {
      if (currentPipelineTimer) {
        clearTimeout(currentPipelineTimer);
        currentPipelineTimer = null;
      }
      currentCardDrawerId = String(id);
      var d = document.getElementById('card-drawer');
      var l = document.getElementById('drawer-card-link');
      var t = document.getElementById('drawer-card-title');
      var m = document.getElementById('drawer-card-meta');
      var domEl = document.getElementById('drawer-card-domains');
      var datesEl = document.getElementById('drawer-card-dates');
      var descCont = document.getElementById('drawer-card-desc-container');
      var descEl = document.getElementById('drawer-card-desc');
      var acCont = document.getElementById('drawer-card-ac-container');
      var acEl = document.getElementById('drawer-card-ac');
      var commentsCont = document.getElementById('drawer-card-comments-container');
      var commentsEl = document.getElementById('drawer-card-comments');
      var cwsEl = document.getElementById('drawer-card-cws');
      var cwsMotion = document.getElementById('drawer-card-cws-motion');
      var cwsInd = document.getElementById('drawer-cws-indicator');

      if (d) d.style.display = 'block';
      if (l) {
        l.href = (window.CLEARING_BASE || '') + '/board?card=' + id;
        l.textContent = '#' + id;
      }
      if (t) t.textContent = 'Loading card #' + id + '...';
      if (m) m.textContent = '';
      if (domEl) { domEl.textContent = ''; domEl.style.display = 'none'; }
      if (datesEl) { datesEl.textContent = ''; datesEl.style.display = 'none'; }
      if (descCont) descCont.style.display = 'none';
      if (descEl) descEl.textContent = '';
      if (acCont) acCont.style.display = 'none';
      if (acEl) acEl.innerHTML = '';
      if (commentsCont) commentsCont.style.display = 'none';
      if (commentsEl) commentsEl.innerHTML = '';
      if (cwsEl) cwsEl.textContent = 'Loading pipeline state...';
      if (cwsMotion) cwsMotion.style.display = 'none';
      if (cwsInd) cwsInd.innerHTML = '<span class="chip">reading pipeline...</span>';

      // 1. Fetch card details first so title, owner, status, description, AC, and comments display immediately
      try {
        var res = await fetch('/api/cards/' + id);
        if (String(id) === currentCardDrawerId) {
          if (res.ok) {
            var json = await res.json();
            if (json.ok && json.card) {
              var c = json.card;
              if (t) t.textContent = c.title;
              var metaParts = [];
              if (c.owner) metaParts.push('Owner: ' + drawerEscape(c.owner));
              if (c.status) metaParts.push('Status: ' + drawerEscape(c.status));
              if (c.priority) metaParts.push('Priority: ' + drawerEscape(c.priority));
              if (m) m.textContent = metaParts.join(' · ');

              if (c.domain && domEl) {
                domEl.textContent = 'Domains: ' + c.domain;
                domEl.style.display = 'block';
              }

              if ((c.created || c.updated) && datesEl) {
                var dateParts = [];
                if (c.created) dateParts.push('Created: ' + drawerEscape(c.created));
                if (c.updated) dateParts.push('Updated: ' + drawerEscape(c.updated));
                datesEl.textContent = dateParts.join(' · ');
                datesEl.style.display = 'block';
              }

              if (c.description && descEl && descCont) {
                descEl.textContent = c.description;
                descCont.style.display = 'block';
              }

              if (c.acItems && c.acItems.length > 0 && acEl && acCont) {
                acEl.innerHTML = c.acItems.map(function(item) {
                  return '<div class="ac-item" style="margin:4px 0;display:flex;align-items:flex-start;gap:6px">' +
                    '<input type="checkbox" disabled ' + (item.checked ? 'checked' : '') + ' style="margin-top:2px">' +
                    '<span style="font-size:12.5px;line-height:1.4' + (item.checked ? ';text-decoration:line-through;color:var(--muted)' : '') + '">' + drawerEscape(item.text) + '</span>' +
                    '</div>';
                }).join('');
                acCont.style.display = 'block';
              }

              if (c.comments && c.comments.length > 0 && commentsEl && commentsCont) {
                commentsEl.innerHTML = c.comments.slice(-3).map(function(item) {
                  var authorText = item.author ? '<b>' + drawerEscape(item.author) + '</b>' : '<b>comment</b>';
                  var timeText = item.time ? ' <span class="meta" style="font-size:11px">' + drawerEscape(item.time) + '</span>' : '';
                  return '<div class="comment-item" style="margin:8px 0;padding:6px 8px;background:var(--bg-primary, rgba(0,0,0,0.03));border:1px solid var(--line2);border-radius:4px;font-size:12.5px;line-height:1.4">' +
                    '<div style="margin-bottom:3px;font-size:11.5px">' + authorText + timeText + '</div>' +
                    '<div style="white-space:pre-wrap">' + drawerEscape(item.text) + '</div>' +
                    '</div>';
                }).join('');
                commentsCont.style.display = 'block';
              }
            }
          } else {
            if (t) t.textContent = 'Card #' + id + ' not found on board';
          }
        }
      } catch (e) {
        if (String(id) === currentCardDrawerId && t) t.textContent = 'Card #' + id;
      }

      // 2. Fetch live pipeline status without blocking card details
      loadCardPipeline(id);
    }

    async function loadCardPipeline(id) {
      if (String(id) !== currentCardDrawerId) return;
      var cwsEl = document.getElementById('drawer-card-cws');
      var cwsMotion = document.getElementById('drawer-card-cws-motion');
      var cwsInd = document.getElementById('drawer-cws-indicator');

      try {
        var res = await fetch('/api/cards/' + id + '/pipeline');
        if (String(id) !== currentCardDrawerId) return;
        if (res.ok) {
          var json = await res.json();
          if (json.ok && json.pipeline) {
            var p = json.pipeline;
            if (cwsEl) cwsEl.textContent = p.output || 'no runs';

            if (p.status === 'running') {
              if (cwsInd) cwsInd.innerHTML = '<span class="chip chip-running"><span class="motion-pulse-dot"></span> running' + (p.step ? ': ' + drawerEscape(p.step) : '') + '</span>';
              if (cwsMotion) {
                cwsMotion.style.display = 'flex';
                cwsMotion.innerHTML = '<span class="motion-pulse-dot"></span><span>Step ' + drawerEscape(p.step || 'running') + ' in motion...</span>';
              }
            } else if (p.status === 'waiting') {
              if (cwsInd) cwsInd.innerHTML = '<span class="chip chip-waiting"><span class="motion-pulse-dot"></span> waiting for go</span>';
              if (cwsMotion) {
                cwsMotion.style.display = 'flex';
                cwsMotion.innerHTML = '<span class="motion-pulse-dot"></span><span>Waiting for your go...</span>';
              }
            } else if (p.status === 'failed') {
              if (cwsInd) cwsInd.innerHTML = '<span class="chip chip-failed">failed' + (p.failedStep ? ' at ' + drawerEscape(p.failedStep) : '') + '</span>';
              if (cwsMotion) cwsMotion.style.display = 'none';
            } else if (p.status === 'unmeasured' || p.status === 'stale' || p.isStale) {
              var staleLabel = (p.isStale || p.status === 'stale') ? 'stale' : 'unmeasured';
              if (cwsInd) cwsInd.innerHTML = '<span class="chip chip-failed">' + staleLabel + (p.unmeasuredReason ? ' (' + drawerEscape(p.unmeasuredReason) + ')' : '') + '</span>';
              if (cwsMotion) cwsMotion.style.display = 'none';
            } else if (p.status === 'landed') {
              if (cwsInd) cwsInd.innerHTML = '<span class="chip chip-ok">landed</span>';
              if (cwsMotion) cwsMotion.style.display = 'none';
            } else if (p.status === 'succeeded') {
              if (cwsInd) cwsInd.innerHTML = '<span class="chip chip-ok">succeeded</span>';
              if (cwsMotion) cwsMotion.style.display = 'none';
            } else {
              if (cwsInd) cwsInd.innerHTML = '<span class="chip">' + drawerEscape(p.status || 'no runs') + '</span>';
              if (cwsMotion) cwsMotion.style.display = 'none';
            }

            if (p.isLive) {
              currentPipelineTimer = setTimeout(function() {
                loadCardPipeline(id);
              }, 2500);
            }
            return;
          }
        }
        if (cwsEl) cwsEl.textContent = 'Pipeline status unreadable';
        if (cwsMotion) cwsMotion.style.display = 'none';
        if (cwsInd) cwsInd.innerHTML = '<span class="chip strip-source-fail">unreadable</span>';
      } catch (err) {
        if (String(id) !== currentCardDrawerId) return;
        if (cwsEl) cwsEl.textContent = 'Failed to read pipeline: ' + err.message;
        if (cwsMotion) cwsMotion.style.display = 'none';
      }
    }

    function closeCardDrawer() {
      if (currentPipelineTimer) {
        clearTimeout(currentPipelineTimer);
        currentPipelineTimer = null;
      }
      currentCardDrawerId = null;
      var d = document.getElementById('card-drawer');
      if (d) d.style.display = 'none';
    }

    function toggleThinking() {
      var cont = document.getElementById('messages-stream');
      var btn = document.getElementById('thinking-toggle-btn');
      if (!cont) return;
      var isHidden = cont.classList.toggle('hide-thinking');
      if (btn) btn.textContent = isHidden ? 'Thinking: OFF' : 'Thinking: ON';
      try {
        localStorage.setItem('clearing_thinking_visible', isHidden ? 'false' : 'true');
      } catch (e) {}
    }

    try {
      if (typeof localStorage !== 'undefined' && localStorage.getItem('clearing_thinking_visible') === 'false') {
        var cont = document.getElementById('messages-stream');
        var btn = document.getElementById('thinking-toggle-btn');
        if (cont) cont.classList.add('hide-thinking');
        if (btn) btn.textContent = 'Thinking: OFF';
      }
    } catch (e) {}

    function openRoleStream(roleId) {
      var drawer = document.getElementById('stream-drawer');
      var title = document.getElementById('stream-drawer-title');
      var status = document.getElementById('stream-drawer-status');
      var cardsEl = document.getElementById('stream-drawer-cards');
      var actionsEl = document.getElementById('stream-drawer-actions');
      if (!drawer) return;

      var actors = window.PULSE_ACTORS || [];
      var actor = actors.find(function(a) { return a.id === roleId; });
      if (!actor) return;

      if (title) title.innerHTML = '<b>' + drawerEscape(actor.name) + ' Stream</b>';
      if (status) status.innerHTML = 'State: <b>' + drawerEscape(actor.state) + '</b> · Activity: <b>' + drawerEscape(actor.activity || 'idle') + '</b>';

      var cards = actor.cards && actor.cards.length > 0 ? actor.cards : (actor.card ? [actor.card] : []);
      if (cardsEl) {
        if (cards.length === 0) {
          cardsEl.innerHTML = '<span class="meta">0 WIP cards</span>';
        } else {
          cardsEl.innerHTML = '<div class="sec-t" style="margin-bottom:4px">WIP Cards</div>' + cards.map(function(c) {
            return '<div><a class="cid" href="/board?card=' + c.id + '">#' + c.id + '</a> ' + drawerEscape(c.title) + '</div>';
          }).join('');
        }
      }

      if (actionsEl) {
        var stream = actor.stream || [];
        if (stream.length === 0) {
          actionsEl.innerHTML = '<div class="meta" style="padding:8px 0">no recent actions recorded</div>';
        } else {
          actionsEl.innerHTML = stream.map(function(s) {
            var timeStr = s.timestamp ? s.timestamp.slice(11, 16) : '';
            return '<div class="stream-item"><span class="stream-ts">' + drawerEscape(timeStr) + '</span> <span class="stream-action">' + drawerEscape(s.action) + '</span></div>';
          }).join('');
        }
      }

      drawer.style.display = 'flex';
    }

    function closeStreamDrawer() {
      var drawer = document.getElementById('stream-drawer');
      if (drawer) drawer.style.display = 'none';
    }
  </script>`;

/**
 * #4639 — Jeff 10-24 04:59: "i dont need the circles for each name i dont need a team tile
 * basically i need to see name and cards for all logged in roles".
 * A role is logged in if its state is not offline/off, and if sessions are reported,
 * at least one session is live (or not all marked "not live").
 */
export function isRoleLoggedIn(a: ActorRow): boolean {
  const state = (a.state || '').toLowerCase();
  if (state === 'offline' || state === 'off') return false;
  if (a.sessions.length > 0) {
    const hasLive = a.sessions.some(s => s.includes(' · live') || s === 'live');
    const allNotLive = a.sessions.every(s => s.startsWith('+') || s.includes('not live'));
    if (allNotLive && !hasLive) return false;
  }
  return true;
}

function railActorsFor(actors: ActorRow[] | undefined): ActorRow[] {
  const list = actors || [];
  const knownRoleSlugs = ['wren', 'silas', 'kade', 'abby-normal'];
  const matched = knownRoleSlugs.map(roleId => {
    const found = list.find(a => a.id.toLowerCase() === roleId || a.name.toLowerCase().replace(/\s+/g, '-') === roleId);
    if (found) return found;
    return {
      id: roleId,
      name: roleId === 'abby-normal' ? 'Abby Normal' : roleId.charAt(0).toUpperCase() + roleId.slice(1),
      state: 'idle',
      sessions: [],
      cards: [],
      stream: [],
    };
  });
  const knownSet = new Set(knownRoleSlugs);
  const extra = list.filter(a => !knownSet.has(a.id.toLowerCase()) && a.id.toLowerCase() !== 'jeff');
  return [...matched, ...extra].filter(isRoleLoggedIn);
}

/** #4639 / #4723 — compact role tiles for logged in roles: no #team tile, no avatar circles;
 *  andon flasher dot beside name + clickable card links in a single row without horizontal overflow. */
export function renderPeopleRow(room: string, actors: ActorRow[] | undefined): string {
  const railActors = railActorsFor(actors);
  return `
    <div class="actor-rail people-row" id="rooms-people-row">
      ${railActors.map(a => {
        const andon = roleAndonLight(a);
        const cardHtml = a.cards && a.cards.length > 0
          ? a.cards.map(c => `<span class="cid card-ref" onclick="event.stopPropagation();if(typeof openCardDrawer==='function')openCardDrawer('${c.id}');return false;">#${c.id}</span>`).join(' ')
          : (a.card ? `<span class="cid card-ref" onclick="event.stopPropagation();if(typeof openCardDrawer==='function')openCardDrawer('${a.card.id}');return false;">#${a.card.id}</span>` : 'no card');
        const isActive = room.toLowerCase() === a.id.toLowerCase();
        return `
          <a class="actor-tile actor-tile-${a.id}${isActive ? ' on' : ''}" data-role="${a.id}" href="/rooms/${a.id}" onclick="if(typeof toggleActorFilter==='function'){return toggleActorFilter(event, '${a.id}');}window.location.href=(window.CLEARING_BASE || '') + '/rooms/${a.id}';return false;">
            <div class="actor-tile-info">
              <div class="actor-tile-name"><span class="andon-dot ${andon.cssClass}"></span><b>${escapeHtml(a.name)}</b></div>
              <div class="actor-tile-card">${cardHtml}</div>
            </div>
          </a>
        `;
      }).join('')}
    </div>`;
}

export function renderRoomsPage(room: string = 'team', data: RoomsData, sources: TrustStripSource[]): string {
  const directRoleMatch = data.directRoles.find(dr => dr.toLowerCase().replace(/\s+/g, '-') === room.toLowerCase());
  const isDirect = Boolean(directRoleMatch);
  const headerTitle = isDirect ? `@ ${directRoleMatch}` : `# ${room}`;

  const railActors = railActorsFor(data.actors);
  const peopleRowHtml = renderPeopleRow(room, data.actors);

  const content = `
  <div class="rooms-view">
    <div class="rooms">
      <main class="chat chat-container" style="flex:1;min-width:0">
        <div class="row chat-header-row" style="justify-content:space-between;padding:4px 10px;border-bottom:1px solid var(--line2);min-width:0;flex-wrap:nowrap;overflow-x:auto">
          <div style="min-width:0;display:inline-flex;align-items:center;gap:6px;flex-shrink:0">
            <select id="room-picker" class="room-picker" onchange="window.location.href=(window.CLEARING_BASE || '') + '/rooms/' + this.value" style="font-weight:700;font-size:13px;background:var(--frame);border:1px solid var(--line2);border-radius:4px;padding:2px 6px;color:var(--ink);cursor:pointer">
              <optgroup label="Rooms">
                ${data.channels.map(c => `<option value="${c}"${room === c ? ' selected' : ''}># ${c}</option>`).join('')}
              </optgroup>
              <optgroup label="Direct">
                ${data.directRoles.map(dr => {
                  const slug = dr.toLowerCase().replace(/\s+/g, '-');
                  return `<option value="${slug}"${room === slug ? ' selected' : ''}>@ ${dr}</option>`;
                }).join('')}
              </optgroup>
            </select>
            <input type="search" id="room-search-input" placeholder="Search loaded..." oninput="if(typeof setSearchQuery==='function')setSearchQuery(this.value);" style="font-size:11px;background:var(--frame);border:1px solid var(--line2);border-radius:4px;padding:2px 6px;color:var(--ink);width:110px">
          </div>
          <div class="row" style="gap:8px;align-items:center;flex-shrink:0;flex-wrap:nowrap">
            <button id="thinking-toggle-btn" class="btn ghost toggle-thinking-btn" onclick="toggleThinking();return false;" style="font-size:11px;padding:2px 6px;cursor:pointer">Thinking: ON</button>
            <label class="meta" style="font-size:11px;white-space:nowrap"><input type="checkbox" id="team-nudges"> nudges</label>
            <a class="meta" href="/counts" style="font-size:11px">counts</a>
            <span class="ex" style="font-size:10.5px">${data.relayConnected ? 'live relay' : 'relay disconnected'}</span>
          </div>
        </div>
        <div id="history-status" class="history-status" style="font-size:11.5px;text-align:center;color:var(--meta, #888);padding:4px 0" role="status"></div>
        <div class="messages" id="messages-stream">
          ${data.messages.length === 0 ? `
            <div class="empty-feed" style="padding:24px;color:var(--meta);text-align:center">
              No messages in ${headerTitle} yet
            </div>
          ` : data.messages.map(renderRoomMessage).join('')}
        </div>
        <footer class="composer-bar">
          <form id="composer-form">
            <textarea id="composer-input" rows="1" placeholder="Message ${isDirect ? '@' : '#'}${room}..." autocomplete="off"></textarea>
            <label for="composer-photo" class="composer-photo" title="Attach a photo" aria-label="Attach a photo"><input type="file" id="composer-photo" accept="image/*" multiple class="sr-only">📷</label>
      <button type="button" class="composer-mic" id="composer-mic" title="Record voice" aria-label="Record voice" aria-pressed="false">🎤</button>
      <button type="submit" id="composer-send">Send</button>
          </form>
          <div id="send-error" class="send-error" role="alert"></div>
        </footer>
        ${peopleRowHtml}
      </main>
      <aside class="stream-drawer" id="stream-drawer" style="display:none">
        <div class="stream-drawer-h">
          <div id="stream-drawer-title"><b>Role Stream</b></div>
          <button class="btn ghost" onclick="closeStreamDrawer();return false;" style="font-size:16px;padding:4px 8px;cursor:pointer">✕</button>
        </div>
        <div id="stream-drawer-status" class="meta" style="margin-bottom:8px"></div>
        <div id="stream-drawer-cards" style="margin-bottom:10px"></div>
        <div class="sec-t" style="margin-bottom:6px">Recent Actions (Pulse Stream)</div>
        <div id="stream-drawer-actions" class="stream-list"></div>
      </aside>
  ${ROOM_CARD_DRAWER}
    </div>
  </div>
  <script>window.PULSE_ACTORS = ${JSON.stringify(railActors)};</script>
${ROOM_DRAWER_SCRIPT}
${TEAM_NUDGES_TOGGLE}`;

  return renderPageShell(`Room #${room}`, 'rooms', content, sources);
}


/**
 * #4592 — today's messages by kind: sent, reached Buzz, gap. A source that
 * cannot be read is named, never shown as zeros (a red must mean the product
 * broke, not that the page could not look).
 */
/** #4634 — a day where Buzz kept fewer than were sent, or the spine read 0 events. */
function redCell(c: DayCell, d: DayRow): boolean {
  const sent = d.values.find((x) => x.metric === 'messages-sent')?.value;
  if (c.metric === 'messages-kept') return c.value !== null && sent !== null && sent !== undefined && c.value < sent;
  return c.metric === 'events' && c.value === 0;
}

/** #4634 — messages and events by day since day 1, from the sensors' readings. */
export function renderCountsPage(days: DayRow[], meta: { error?: string }, sources: TrustStripSource[], doubles: DayRow[] = []): string {
  const cell = (v: number | null, cls = '') => `<td class="num ${cls}">${v === null ? '<span class="meta">unmeasured</span>' : v}</td>`;
  // #4628 — the doubles table goes first: under 200+ day rows Jeff would never reach it (Wren, 390 px)
  const doublesHtml = doubles.length === 0
      ? '<p class="meta">Doubles: unmeasured (the sensor has no doubles reading yet)</p>'
      : `<h3 style="margin-top:16px">Doubles in Buzz <span class="meta">same author, same words, within 20 s</span></h3>
    <table class="counts-table" style="width:100%;border-collapse:collapse">
      <thead><tr><th style="text-align:left">Day</th>${DOUBLE_COLUMNS.map(([, , l]) => `<th>${l}</th>`).join('')}</tr></thead>
      <tbody>
${doubles.map((d) => `        <tr><td>${d.day.slice(5)}</td>${d.values.map((c) => cell(c.value, c.value !== null && c.value > 0 ? 'fail' : '')).join('')}</tr>`).join('\n')}
      </tbody>
    </table>
    <p class="meta">Each kind of message has one writer, so any double is a second writer still in place.</p>`;
  const body = meta.error || days.length === 0
    ? `<div class="meta fail" style="padding:16px">Unmeasured: ${escapeHtml(meta.error || 'the sensors have written no readings yet')}</div>`
    : `<div style="overflow-x:auto"><table class="counts-table" style="width:100%;border-collapse:collapse">
      <thead><tr><th style="text-align:left">Day</th>${DAY_COLUMNS.map(([, , l]) => `<th>${l}</th>`).join('')}</tr></thead>
      <tbody>
${days.map((d) => `        <tr><td>${d.day}</td>${d.values.map((c) => cell(c.value, redCell(c, d) ? 'fail' : '')).join('')}</tr>`).join('\n')}
      </tbody>
    </table></div>
    <p class="meta" style="margin-top:12px">From the herald sensors (messages: pulse and Buzz; events: the spine). A day is counted once it is over.</p>`;
  const content = `<main class="page counts-page" style="padding:14px">
    <p class="meta"><a href="/sync">Git vs graph by day</a> (the crawler's sync sensor)</p>
    ${doublesHtml}
    <h2>Messages and events by day <span class="meta">Boston days</span></h2>
    ${body}
  </main>`;
  return renderPageShell('Messages and events', 'counts', content, sources);
}

/**
 * #4647 — git vs graph, from the sync sensor: per crawled class, files in git
 * with no row (missing) and rows out of date (stale). Summary first, the
 * newest day's classes worst first, then the history, at 390 px (Wren 18:39).
 * A class or a day with no reading says unmeasured, never 0.
 */
export function renderSyncPage(days: SyncDay[], meta: { error?: string }, sources: TrustStripSource[]): string {
  const num = (v: number | null) => `<td class="num ${v !== null && v > 0 ? 'fail' : ''}">${v === null ? '<span class="meta">unmeasured</span>' : v}</td>`;
  let body: string;
  if (meta.error || days.length === 0) {
    body = `<div class="meta fail" style="padding:16px">Unmeasured: ${escapeHtml(meta.error || 'the sync sensor has written no readings yet')}</div>`;
  } else {
    const latest = days[0];
    const unmeasured = latest.classes.filter((c) => c.missing === null || c.stale === null);
    const off = latest.classes.filter((c) => (c.missing || 0) + (c.stale || 0) > 0);
    const summary = unmeasured.length
      ? `<p class="fail"><b>${latest.day}: unmeasured</b> for ${unmeasured.map((c) => escapeHtml(c.subject)).join(', ')}</p>`
      : off.length
        ? `<p class="fail"><b>${latest.day}: ${off.length} of ${latest.classes.length} classes out of sync</b>, ${off.reduce((n, c) => n + (c.missing || 0), 0)} missing, ${off.reduce((n, c) => n + (c.stale || 0), 0)} stale</p>`
        : `<p><b>${latest.day}: all ${latest.classes.length} classes in sync</b></p>`;
    const history = days.slice(1).map((d) => {
      const bad = d.classes.filter((c) => c.missing === null || c.stale === null || (c.missing || 0) + (c.stale || 0) > 0);
      return `        <tr><td>${d.day.slice(5)}</td><td class="${bad.length ? 'fail' : ''}">${bad.length ? bad.map((c) => escapeHtml(c.subject)).join(', ') : 'in sync'}</td></tr>`;
    }).join('\n');
    body = `${summary}
    <table class="counts-table" style="width:100%;border-collapse:collapse">
      <thead><tr><th style="text-align:left">Class</th><th>Missing</th><th>Stale</th></tr></thead>
      <tbody>
${latest.classes.map((c) => `        <tr><td>${escapeHtml(c.subject)}</td>${num(c.missing)}${num(c.stale)}</tr>`).join('\n')}
      </tbody>
    </table>
    <p class="meta">Missing: files in git with no row in the graph. Stale: rows older than the file. The sensor runs after every land crawl and at 03:30.</p>${history ? `
    <h3 style="margin-top:16px">Earlier days</h3>
    <table class="counts-table" style="width:100%;border-collapse:collapse">
      <thead><tr><th style="text-align:left">Day</th><th style="text-align:left">Out of sync</th></tr></thead>
      <tbody>
${history}
      </tbody>
    </table>` : ''}`;
  }
  const content = `<main class="page counts-page" style="padding:14px">
    <h2>Git vs graph <span class="meta">Boston days</span></h2>
    ${body}
    <p class="meta" style="margin-top:12px"><a href="/counts">Messages and events by day</a></p>
  </main>`;
  return renderPageShell('Git vs graph', 'counts', content, sources);
}

/**
 * #4597 — Jeff 16:25: "i really need a streams menu option too". One column per role,
 * newest command first, refreshed every 5 seconds. An unreadable stream is named.
 */
export function renderStreamsPage(streams: Streams, sources: TrustStripSource[]): string {
  const body = !streams.ok
    ? `<div class="meta fail" style="padding:16px">Streams could not be read: ${escapeHtml(streams.reason)}</div>`
    : streams.roles.length === 0
      ? `<div class="meta" style="padding:16px">No commands in the stream yet.</div>`
      : `<div class="streams-grid">${streams.roles.map((r) => `
      <section class="stream-col">
        <h3 class="who-${escapeHtml(r.role)}">${escapeHtml(r.role)}</h3>
        ${r.lines.map((l) => `<div class="stream-line"><span class="tm">${escapeHtml(l.ts)}</span> ${escapeHtml(l.text)}</div>`).join('')}
      </section>`).join('')}
    </div>`;
  const content = `<main class="page streams-page" id="streams-page">
    ${body}
  </main>
  <script>setTimeout(function () { window.location.reload(); }, 5000);</script>`;
  return renderPageShell('Streams', 'streams', content, sources);
}

function escapeHtml(t: string | undefined): string {
  if (!t) return '';
  return t.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

/** #4574 — the Board tab: the board API's columns as served. A column the API cannot
 *  give says unmeasured with the reason; v2 sorts nothing into columns itself. */
export function renderBoardPage(columns: BoardColumn[], openCard: number | null, sources: TrustStripSource[], rolesData?: RolesData): string {
  const col = (c: BoardColumn) => `
      <section class="board-col">
        <h3>${c.name}${c.ok ? ` <span class="meta">${c.cards.length}</span>` : ''}</h3>
        ${c.ok
          ? (c.cards.length === 0
            ? '<div class="meta">0 cards</div>'
            : c.cards.map((k) => {
              const presence = ownerPresence(k.owner, rolesData);
              return `
            <div class="board-card" data-open-card="${k.id}" onclick="openCardDrawer('${k.id}');return false;">
              <div class="task-id"><a class="cid card-ref" href="#" onclick="openCardDrawer('${k.id}');return false;">#${k.id}</a></div>
              <div class="board-card-title">${escapeHtml(k.title)}</div>
              ${k.owner ? `
                <div class="board-card-meta">
                  <span class="card-owner-presence"><span class="dot ${presence.dotClass}"></span> <span>${escapeHtml(k.owner)}</span> · <span class="presence-label presence-${presence.status}">${presence.text}</span></span>
                </div>` : ''}
            </div>`;
            }).join(''))
          : renderUnmeasured('board-unmeasured', c.reason)}
      </section>`;
  const content = `
  <div class="page board-page" style="display:flex">
    <div style="flex:1">
      <div class="ph"><h2>Board</h2><span class="ex">chorus board API</span></div>
      <div class="board-cols">${columns.map(col).join('')}</div>
    </div>
${ROOM_CARD_DRAWER}
  </div>
${ROOM_DRAWER_SCRIPT}${openCard ? `  <script>openCardDrawer('${openCard}');</script>\n` : ''}`;
  return renderPageShell('Board', 'board', content, sources);
}
