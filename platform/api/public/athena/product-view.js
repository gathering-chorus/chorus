const ATHENA = '/api/athena';
const ownerClasses = { Jeff: 'jeff', Wren: 'wren', Silas: 'silas', Kade: 'kade' };
const stepClasses = { Shaping: 'step-shaping', Directing: 'step-directing', Designing: 'step-designing', Building: 'step-building', Proving: 'step-proving' };

let activeOwner = 'All';

async function init() {
  const container = document.getElementById('content');
  try {
    const [spRes, sdRes] = await Promise.all([
      fetch(ATHENA + '/subproducts'),
      fetch(ATHENA + '/subdomains'),
    ]);
    if (!spRes.ok || !sdRes.ok) throw new Error('Athena error');
    const spBody = await spRes.json();
    const sdBody = await sdRes.json();

    const subproducts = spBody.data;
    const subdomains = sdBody.data;

    // Attach subdomains to subproducts by checking which subdomains belong to which subproduct
    // For now, group subdomains by owner since partOf edges aren't populated yet
    window._subproducts = subproducts;
    window._subdomains = subdomains;

    renderFilters();
    renderContent();
  } catch (err) {
    container.innerHTML = '<div class="error">Could not load from Athena: ' + err.message + '</div>';
  }
}

function renderFilters() {
  const owners = ['All', 'Jeff', 'Wren', 'Silas', 'Kade'];
  const container = document.getElementById('owner-filters');
  if (!container) return;
  container.innerHTML = '<span class="filter-label">Owner</span>' + owners.map(o => {
    const isActive = o === activeOwner;
    const dot = o !== 'All' ? '<span class="owner-dot ' + (ownerClasses[o] || '') + '"></span>' : '';
    return '<button class="filter-btn' + (isActive ? ' active' : '') + '" data-owner="' + o + '">' + dot + o + '</button>';
  }).join('');
  container.querySelectorAll('.filter-btn').forEach(btn => {
    btn.addEventListener('click', () => { activeOwner = btn.dataset.owner; renderFilters(); renderContent(); });
  });
}

function renderContent() {
  const container = document.getElementById('content');
  let sps = window._subproducts || [];
  if (activeOwner !== 'All') {
    sps = sps.filter(sp => sp.owner === activeOwner);
  }

  if (sps.length === 0) {
    container.innerHTML = '<div class="empty-state">No sub-products match the current filters</div>';
    return;
  }

  let html = '<div class="subproduct-grid">';
  sps.forEach(sp => {
    html += '<div class="subproduct-card">';
    html += '<h3><span class="owner-dot ' + (ownerClasses[sp.owner] || '') + '"></span>' + sp.label + '</h3>';
    html += '<div class="meta">' + (sp.owner || 'Unknown') + '</div>';
    html += '<div class="stats-row">';
    html += '<div class="stat-mini"><div class="val">' + sp.domainCount + '</div><div class="lbl">Domains</div></div>';
    html += '<div class="stat-mini"><div class="val">' + sp.consumesCount + '</div><div class="lbl">Consumes</div></div>';
    html += '</div>';
    html += '</div>';
  });
  html += '</div>';

  container.innerHTML = html;
}

init();
