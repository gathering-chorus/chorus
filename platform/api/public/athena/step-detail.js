const ATHENA = '/api/athena';
const stepClasses = { Shaping: 'step-shaping', Directing: 'step-directing', Designing: 'step-designing', Building: 'step-building', Proving: 'step-proving', Capturing: 'step-capturing' };
const ownerClasses = { Jeff: 'jeff', Wren: 'wren', Silas: 'silas', Kade: 'kade' };

const params = new URLSearchParams(window.location.search);
const stepName = params.get('step') || 'Shaping';

let allSteps = [];
let currentStep = null;
let currentIdx = 0;
let activeFilter = 'all';

async function init() {
  try {
    const res = await fetch(ATHENA + '/steps');
    if (!res.ok) throw new Error('Athena ' + res.status);
    const body = await res.json();
    allSteps = body.data;

    currentIdx = allSteps.findIndex(s => s.label.toLowerCase() === stepName.toLowerCase());
    if (currentIdx < 0) currentIdx = 0;
    currentStep = allSteps[currentIdx];

    // Fetch domains for this step
    const domRes = await fetch(ATHENA + '/subdomains?step=' + currentStep.label.toLowerCase());
    if (!domRes.ok) throw new Error('Athena subdomains ' + domRes.status);
    const domBody = await domRes.json();
    currentStep.subdomains = domBody.data;

    renderPage();
  } catch (err) {
    document.getElementById('domain-grid').innerHTML = '<div class="error">Could not load from Athena: ' + err.message + '</div>';
  }
}

function renderPage() {
  const step = currentStep;
  document.getElementById('step-badge').textContent = step.label;
  document.getElementById('step-badge').className = 'step-badge ' + (stepClasses[step.label] || '');
  document.getElementById('step-title').textContent = 'Domains';
  document.getElementById('breadcrumb-step').textContent = step.label;
  document.getElementById('step-subtitle').textContent = step.subdomains.length + ' domain' + (step.subdomains.length !== 1 ? 's' : '') + ' in the ' + step.label + ' step';
  document.title = step.label + ' — Athena';

  document.getElementById('stat-domains').textContent = step.subdomains.length;
  const uniqueOwners = [...new Set(step.subdomains.map(d => d.owner))];
  document.getElementById('stat-owners').textContent = uniqueOwners.length;

  // Navigation
  if (currentIdx > 0) {
    const prev = document.getElementById('prev-step');
    prev.href = 'step-detail.html?step=' + allSteps[currentIdx - 1].label;
    prev.textContent = '\u2190 ' + allSteps[currentIdx - 1].label;
    prev.classList.remove('disabled');
  }
  if (currentIdx < allSteps.length - 1) {
    const next = document.getElementById('next-step');
    next.href = 'step-detail.html?step=' + allSteps[currentIdx + 1].label;
    next.textContent = allSteps[currentIdx + 1].label + ' \u2192';
    next.classList.remove('disabled');
  }

  renderFilters(uniqueOwners);
  renderDomains();
}

function renderFilters(uniqueOwners) {
  const container = document.getElementById('filters');
  const owners = ['All', ...uniqueOwners];
  container.innerHTML = owners.map(o => {
    const isActive = (o === 'All' && activeFilter === 'all') || o.toLowerCase() === activeFilter;
    const dot = o !== 'All' ? '<span class="owner-dot ' + (ownerClasses[o] || '') + '"></span>' : '';
    return '<button class="filter-btn' + (isActive ? ' active' : '') + '" data-owner="' + (o === 'All' ? 'all' : o.toLowerCase()) + '">' + dot + o + '</button>';
  }).join('');
  container.querySelectorAll('.filter-btn').forEach(btn => {
    btn.addEventListener('click', () => {
      activeFilter = btn.dataset.owner;
      renderFilters(uniqueOwners);
      renderDomains();
    });
  });
}

function renderDomains() {
  const grid = document.getElementById('domain-grid');
  const domains = currentStep.subdomains;
  const filtered = activeFilter === 'all' ? domains : domains.filter(d => d.owner && d.owner.toLowerCase() === activeFilter);
  if (filtered.length === 0) {
    grid.innerHTML = '<div class="empty-state">No domains' + (activeFilter !== 'all' ? ' for this owner' : ' in this step') + '</div>';
    return;
  }
  grid.innerHTML = filtered.map(d => {
    const oc = ownerClasses[d.owner] || '';
    return '<a class="domain-card" href="../domain.html?id=' + d.id + '" data-domain-id="' + d.id + '">' +
      '<h3>' + d.label + '</h3>' +
      '<div class="meta"><span class="owner-dot ' + oc + '"></span>' + d.owner + '</div>' +
      '<div class="domain-stats" style="display:flex;gap:8px;margin:8px 0 4px;flex-wrap:wrap"></div>' +
      '<span class="type-badge">Sub-Domain</span>' +
      '</a>';
  }).join('');

  // Enrich cards with test + code counts (#2098)
  filtered.forEach(function(d) {
    var domainName = d.id.replace(/-(?:domain|service|analytics)$/, '');
    var card = grid.querySelector('[data-domain-id="' + d.id + '"]');
    if (!card) return;
    var statsEl = card.querySelector('.domain-stats');
    // Fetch test count
    fetch('/api/quality/domain/' + domainName)
      .then(function(r) { return r.ok ? r.json() : null; })
      .then(function(data) {
        if (!data || !statsEl) return;
        if (data.total > 0) {
          statsEl.innerHTML += '<span style="font-size:0.8em;background:#d1fae5;color:#065f46;padding:2px 8px;border-radius:4px;">' + data.total + ' tests</span>';
        }
        if (data.files && data.files.length > 0) {
          statsEl.innerHTML += '<span style="font-size:0.8em;background:#dbeafe;color:#1e40af;padding:2px 8px;border-radius:4px;">' + data.files.length + ' files</span>';
        }
      })
      .catch(function() {});
    // Fetch code count
    fetch('/api/chorus/domain/' + d.id + '/code')
      .then(function(r) { return r.ok ? r.json() : null; })
      .then(function(data) {
        if (!data || !statsEl) return;
        var files = (data.data && data.data.files) || [];
        if (files.length > 0) {
          statsEl.innerHTML += '<span style="font-size:0.8em;background:#fef3c7;color:#92400e;padding:2px 8px;border-radius:4px;">' + files.length + ' source</span>';
        }
      })
      .catch(function() {});
  });
}

init();
