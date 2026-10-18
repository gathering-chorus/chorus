import type { BoardColumn } from './sources/board';
// @domain: messages

import { TrustStripSource, RolesData, TodayData, RoomsData, ActorRow } from './data-service';

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
    <a href="/calendar" class="${on('calendar')}">Calendar</a>
    <a href="/plans" class="${on('plans')}">Plans</a>
    <a href="/alerts" class="${on('alerts')}">Alerts${countBadge(counts?.alerts)}</a>
  </nav>`;
}

/** #4575 — Jeff 07:01: "i dont see a way to send now in board". Every page carries the
 *  #team composer (Rooms has its own, per room); no @ goes to Wren, as in the room. */
const TEAM_COMPOSER = `<footer class="composer-bar page-composer">
    <form id="composer-form">
      <input type="text" id="composer-input" placeholder="Message #team..." autocomplete="off">
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
  <link rel="stylesheet" href="/style.css">
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
  <script type="module" src="/room-key.js"></script>
</head>
<body>
  <header class="app-header">
    <div class="header-left">
      <h1>The Clearing <span class="badge-v2">v2</span></h1>
      <span class="relay-status" id="relay-indicator">Connecting...</span>
    </div>
  </header>
  ${renderNav(activeNav, counts)}
  <div class="strip trust-strip">
    ${chipsHtml}
  </div>
  ${bodyContent}
  ${activeNav === 'rooms' ? '' : TEAM_COMPOSER}
  <script src="/app.js"></script>
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

function renderCardRef(card: ActorRow['card']): string {
  if (!card) return '<span class="meta">–</span>';
  return `<a class="cid" href="/board?card=${card.id}">#${card.id}</a>${card.title ? ` <span class="meta">${card.title}</span>` : ''}`;
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
                <div class="meta">${w.card ? renderCardRef(w.card) : 'no current card'}</div>
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
                <td class="meta">${a.activity || '–'}</td>
                <td>${renderCardRef(a.card)}</td>
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

function formatRoomText(raw: string): string {
  return escapeHtml(raw)
    .replace(/`([^`\n]+)`/g, '<code>$1</code>')
    .replace(/\*\*([^*\n]+)\*\*/g, '<strong>$1</strong>')
    .replace(/#(\d+)/g, '<a class="cid card-ref" href="#" onclick="openCardDrawer(\'$1\');return false;">#$1</a>');
}

function renderRoomMessage(m: RoomsData['messages'][number]): string {
          const linkifiedText = formatRoomText(m.text);
          return `
            <div class="msg" id="msg-${m.id}">
              <span class="av a-${m.from}">${m.from.charAt(0).toUpperCase()}</span>
              <div class="body">
                <span class="nm who-${m.from}">${m.from}</span>
                <span class="tm">${m.time}</span>
                ${m.tick ? '<span class="tick delivery-tick">✓ relay</span>' : ''}
                <div>${linkifiedText}</div>
                ${m.threadCount ? `
                  <div class="thread-container" style="margin-top:6px">
                    <span class="thr">${m.threadCount} working replies${m.threadLast ? ` · last ${m.threadLast}` : ''}</span>
                  </div>
                ` : ''}
              </div>
            </div>
          `;
}

const ROOM_CARD_DRAWER = `    <aside class="card-drawer" id="card-drawer" style="display:none">
      <div class="row" style="justify-content:space-between;align-items:center">
        <a class="cid" id="drawer-card-link" href="#" target="_blank"></a>
        <button class="btn ghost drawer-close-btn" onclick="closeCardDrawer()" style="font-size:16px;padding:4px 8px;cursor:pointer">✕</button>
      </div>
      <h3 id="drawer-card-title" style="margin-top:10px">Loading card...</h3>
      <div class="meta" id="drawer-card-meta" style="margin-top:6px"></div>

      <div id="drawer-card-desc-container" style="display:none;margin-top:14px;border-top:1px solid var(--line2);padding-top:10px">
        <div class="sec-t" style="margin-bottom:6px">Description</div>
        <div class="card-desc-body" id="drawer-card-desc" style="font-size:13px;line-height:1.45;white-space:pre-wrap;color:var(--ink-secondary, inherit)"></div>
      </div>

      <div id="drawer-card-ac-container" style="display:none;margin-top:14px;border-top:1px solid var(--line2);padding-top:10px">
        <div class="sec-t" style="margin-bottom:6px">Acceptance Criteria</div>
        <div class="card-ac-list" id="drawer-card-ac"></div>
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
      var descCont = document.getElementById('drawer-card-desc-container');
      var descEl = document.getElementById('drawer-card-desc');
      var acCont = document.getElementById('drawer-card-ac-container');
      var acEl = document.getElementById('drawer-card-ac');
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
      if (descCont) descCont.style.display = 'none';
      if (descEl) descEl.textContent = '';
      if (acCont) acCont.style.display = 'none';
      if (acEl) acEl.innerHTML = '';
      if (cwsEl) cwsEl.textContent = 'Loading pipeline state...';
      if (cwsMotion) cwsMotion.style.display = 'none';
      if (cwsInd) cwsInd.innerHTML = '<span class="chip">reading pipeline...</span>';

      // 1. Fetch card details first so title, owner, status, description, and AC display immediately
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
  </script>`;

export function renderRoomsPage(room: string = 'team', data: RoomsData, sources: TrustStripSource[]): string {
  const directRoleMatch = data.directRoles.find(dr => dr.toLowerCase().replace(/\s+/g, '-') === room.toLowerCase());
  const isDirect = Boolean(directRoleMatch);
  const headerTitle = isDirect ? `@ ${directRoleMatch}` : `# ${room}`;

  const content = `
  <div class="rooms">
    <aside class="rside rooms-sidebar">
      <div class="lab">Rooms</div>
      ${data.channels.map(c => `
        <div class="r ${room === c ? 'on' : ''}"><a href="/rooms/${c}" style="text-decoration:none;color:inherit"># ${c}</a></div>
      `).join('')}
      <div class="lab">Direct</div>
      ${data.directRoles.map(dr => {
        const slug = dr.toLowerCase().replace(/\s+/g, '-');
        const isActive = room === slug;
        return `<div class="r ${isActive ? 'on' : ''}"><a href="/rooms/${slug}" style="text-decoration:none;color:inherit">${dr}</a></div>`;
      }).join('')}
    </aside>
    <main class="chat chat-container">
      <div class="row" style="justify-content:space-between;padding:10px 14px;border-bottom:1px solid var(--line2)">
        <div><b>${headerTitle}</b> <span class="meta">${data.directRoles.join(', ')}</span></div>
        <span class="ex">${data.relayConnected ? 'live relay' : 'relay disconnected'}</span>
      </div>
      <div class="messages" id="messages-stream">
        ${data.messages.length === 0 ? `
          <div class="empty-feed" style="padding:24px;color:var(--meta);text-align:center">
            No messages in ${headerTitle} yet
          </div>
        ` : data.messages.map(renderRoomMessage).join('')}
      </div>
      <footer class="composer-bar">
        <form id="composer-form">
          <input type="text" id="composer-input" placeholder="Message ${isDirect ? '@' : '#'}${room}..." autocomplete="off">
          <button type="submit" id="composer-send">Send</button>
        </form>
        <div id="send-error" class="send-error" role="alert"></div>
      </footer>
    </main>
${ROOM_CARD_DRAWER}
  </div>
${ROOM_DRAWER_SCRIPT}`;

  return renderPageShell(`Room #${room}`, 'rooms', content, sources);
}


function escapeHtml(t: string): string {
  return t.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

/** #4574 — the Board tab: the board API's columns as served. A column the API cannot
 *  give says unmeasured with the reason; v2 sorts nothing into columns itself. */
export function renderBoardPage(columns: BoardColumn[], openCard: number | null, sources: TrustStripSource[]): string {
  const col = (c: BoardColumn) => `
      <section class="board-col">
        <h3>${c.name}${c.ok ? ` <span class="meta">${c.cards.length}</span>` : ''}</h3>
        ${c.ok
          ? (c.cards.length === 0
            ? '<div class="meta">0 cards</div>'
            : c.cards.map((k) => `<div class="board-card" data-open-card="${k.id}" onclick="openCardDrawer('${k.id}');return false;"><a class="cid card-ref" href="#" onclick="openCardDrawer('${k.id}');return false;">#${k.id}</a> ${escapeHtml(k.title)}${k.owner ? ` <span class="meta">${escapeHtml(k.owner)}</span>` : ''}</div>`).join(''))
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
