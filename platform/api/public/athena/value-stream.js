const ATHENA = '/api/athena';
const stepClasses = {
  Shaping: 'step-shaping', Directing: 'step-directing', Designing: 'step-designing',
  Building: 'step-building', Proving: 'step-proving',
  Sowing: 'step-capturing', Growing: 'step-shaping', Practicing: 'step-directing',
  Harvesting: 'step-building', Reflecting: 'step-proving', Tending: 'step-shaping',
};

// Value stream membership — ordered, not alphabetical
const streams = {
  chorus: { label: 'Chorus', steps: ['Shaping', 'Directing', 'Designing', 'Building', 'Proving'] },
  gathering: { label: 'Gathering — Personal', steps: ['Sowing', 'Growing', 'Practicing', 'Harvesting', 'Reflecting'] },
  life: { label: 'Gathering — Life', steps: ['Tending'] },
};

function ownerDots(subdomains) {
  return subdomains.map(sd => {
    const owner = (sd.owner || '').toLowerCase();
    return '<span class="owner-dot ' + owner + '" title="' + sd.owner + '"></span>';
  }).join('');
}

let allSteps = [];
let activeFilter = 'all';

function renderSteps() {
  const container = document.getElementById('steps-container') || document.body;

  // Determine which streams to show
  const visibleStreams = activeFilter === 'all'
    ? Object.keys(streams)
    : activeFilter === 'chorus'
      ? ['chorus']
      : ['gathering', 'life'];

  // Update filter buttons
  document.querySelectorAll('.filter-btn').forEach(btn => {
    btn.classList.toggle('active', btn.dataset.filter === activeFilter);
  });

  // Count subdomains for the active filter
  const visibleStepNames = visibleStreams.flatMap(s => streams[s].steps);
  const visibleSteps = allSteps.filter(s => visibleStepNames.includes(s.label));
  const totalDomains = visibleSteps.reduce((sum, s) => sum + s.domainCount, 0);

  const el = id => document.getElementById(id);
  if (el('stat-subdomains')) el('stat-subdomains').textContent = totalDomains;
  if (el('stat-steps')) el('stat-steps').textContent = visibleSteps.filter(s => s.domainCount > 0).length;

  let html = '';
  visibleStreams.forEach(streamKey => {
    const stream = streams[streamKey];
    // Preserve hardcoded order — map stream.steps to API data
    const stepMap = Object.fromEntries(allSteps.map(s => [s.label, s]));
    const streamSteps = stream.steps.map(name => stepMap[name] || { label: name, domainCount: 0, subdomains: [] }).filter(Boolean);
    if (streamSteps.length === 0) return;

    html += '<div class="stream-group">';
    html += '<h2 class="stream-title">' + stream.label + '</h2>';
    html += '<div class="stream-steps">';
    streamSteps.forEach((s, i) => {
      const cls = stepClasses[s.label] || 'step-shaping';
      const arrow = i < streamSteps.length - 1 ? '<div class="arrow-sep">&rarr;</div>' : '';
      const product = streamKey === 'chorus' ? 'chorus' : 'gathering';
      html += '<a class="stream-step ' + cls + '" href="step-detail.html?step=' + s.label + '&product=' + product + '">' +
        '<div class="step-tile">' +
        '<h3>' + s.label + '</h3>' +
        '<div class="domain-count">' + s.domainCount + '</div>' +
        '<div class="domain-label">domain' + (s.domainCount !== 1 ? 's' : '') + '</div>' +
        '<div class="owner-dots">' + ownerDots(s.subdomains || []) + '</div>' +
        '</div>' +
        '</a>' + arrow;
    });
    html += '</div></div>';
  });

  container.innerHTML = html;
}

async function init() {
  const container = document.getElementById('steps-container') || document.body;
  try {
    const [stepsRes, productsRes, subproductsRes, subdomainsRes] = await Promise.all([
      fetch(ATHENA + '/steps'),
      fetch(ATHENA + '/products'),
      fetch(ATHENA + '/subproducts'),
      fetch(ATHENA + '/subdomains'),
    ]);

    const stepsBody = await stepsRes.json();
    allSteps = stepsBody.data;

    const productsBody = await productsRes.json();
    const subproductsBody = await subproductsRes.json();
    const subdomainsBody = await subdomainsRes.json();

    const el = id => document.getElementById(id);
    if (el('stat-products')) el('stat-products').textContent = productsBody._meta.count;
    if (el('stat-subproducts')) el('stat-subproducts').textContent = subproductsBody._meta.count;

    // Add product filter bar
    const filterBar = document.getElementById('product-filter');
    if (filterBar) {
      filterBar.innerHTML =
        '<button class="filter-btn active" data-filter="all">All Streams</button>' +
        '<button class="filter-btn" data-filter="chorus">Chorus</button>' +
        '<button class="filter-btn" data-filter="gathering">Gathering</button>';

      filterBar.addEventListener('click', e => {
        const btn = e.target.closest('.filter-btn');
        if (!btn) return;
        activeFilter = btn.dataset.filter;
        renderSteps();
      });
    }

    // Check URL for product param
    const params = new URLSearchParams(window.location.search);
    const productParam = params.get('product');
    if (productParam === 'chorus' || productParam === 'gathering') {
      activeFilter = productParam;
    }

    renderSteps();
  } catch (err) {
    container.innerHTML = '<div class="error">Could not load from Athena: ' + err.message + '</div>';
  }
}

init();
