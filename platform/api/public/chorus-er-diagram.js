// chorus-er-diagram.js — Dynamic ER diagram from Fuseki ontology (#2107)
mermaid.initialize({ startOnLoad: false, theme: 'neutral', er: { useMaxWidth: false, fontSize: 12 } });

var FUSEKI = 'http://localhost:3030/pods/sparql';
var allClasses = {};
var allObjProps = [];
var productDomains = {};

function sparql(query) {
  var cleaned = query.split('\n').map(function(l) { return l.trim(); }).filter(function(l) { return l; }).join('\n');
  return fetch(FUSEKI, {
    method: 'POST',
    headers: { 'Accept': 'application/sparql-results+json', 'Content-Type': 'application/sparql-query' },
    body: cleaned
  }).then(function(res) {
    if (!res.ok) throw new Error('SPARQL ' + res.status);
    return res.json();
  }).then(function(d) {
    return d.results.bindings;
  });
}

function local(uri) { return uri ? uri.split('#').pop() : ''; }

function loadOntology() {
  return Promise.all([
    sparql('PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>\nPREFIX owl: <http://www.w3.org/2002/07/owl#>\nSELECT DISTINCT ?class ?label ?parent ?comment WHERE {\nGRAPH <urn:chorus:ontology> {\n?class a owl:Class .\nOPTIONAL { ?class rdfs:label ?label }\nOPTIONAL { ?class rdfs:subClassOf ?parent . ?parent a owl:Class }\nOPTIONAL { ?class rdfs:comment ?comment }\n}\n}'),
    sparql('PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>\nPREFIX owl: <http://www.w3.org/2002/07/owl#>\nSELECT DISTINCT ?prop ?domain ?range WHERE {\nGRAPH <urn:chorus:ontology> {\n?prop a owl:ObjectProperty ; rdfs:domain ?domain ; rdfs:range ?range .\n?domain a owl:Class . ?range a owl:Class .\n}\n}'),
    sparql('PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>\nPREFIX owl: <http://www.w3.org/2002/07/owl#>\nSELECT DISTINCT ?prop ?domain ?range WHERE {\nGRAPH <urn:chorus:ontology> {\n?prop a owl:DatatypeProperty ; rdfs:domain ?domain .\nOPTIONAL { ?prop rdfs:range ?range }\n?domain a owl:Class .\n}\n}'),
    sparql('PREFIX chorus: <https://chorus.jeffbridwell.com/ontology#>\nPREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>\nSELECT DISTINCT ?product ?label ?domain WHERE {\nGRAPH <urn:chorus:ontology> {\n?product a chorus:Product .\nOPTIONAL { ?product rdfs:label ?label }\nOPTIONAL { ?product chorus:hasDomain ?domain }\n}\n}')
  ]).then(function(results) {
    var classesRaw = results[0], objRaw = results[1], dataRaw = results[2], prodRaw = results[3];

    classesRaw.forEach(function(b) {
      var name = local(b.class.value);
      if (!allClasses[name]) {
        allClasses[name] = {
          label: b.label ? b.label.value : name,
          parent: b.parent ? local(b.parent.value) : null,
          comment: b.comment ? b.comment.value : '',
          attrs: []
        };
      }
    });

    dataRaw.forEach(function(b) {
      var domain = local(b.domain.value);
      var prop = local(b.prop.value);
      var dtype = b.range ? local(b.range.value) : 'string';
      if (allClasses[domain]) allClasses[domain].attrs.push({ type: dtype, name: prop });
    });

    var seen = {};
    objRaw.forEach(function(b) {
      var from = local(b.domain.value);
      var to = local(b.range.value);
      var prop = local(b.prop.value);
      var key = from + '-' + prop + '-' + to;
      if (!seen[key] && allClasses[from] && allClasses[to]) {
        seen[key] = true;
        allObjProps.push({ from: from, to: to, label: prop });
      }
    });

    Object.keys(allClasses).forEach(function(name) {
      var c = allClasses[name];
      if (c.parent && allClasses[c.parent]) {
        var key = name + '-inherits-' + c.parent;
        if (!seen[key]) {
          seen[key] = true;
          allObjProps.push({ from: name, to: c.parent, label: 'inherits' });
        }
      }
    });

    prodRaw.forEach(function(b) {
      var label = b.label ? b.label.value : local(b.product.value);
      var domain = b.domain ? local(b.domain.value) : null;
      if (!productDomains[label]) productDomains[label] = [];
      if (domain) productDomains[label].push(domain);
    });

    var select = document.getElementById('productFilter');
    Object.keys(productDomains).sort().forEach(function(name) {
      var opt = document.createElement('option');
      opt.value = name;
      opt.textContent = name;
      select.appendChild(opt);
    });
  });
}

function buildMermaid(filterProduct) {
  var activeClasses = new Set();
  var activeRels = allObjProps;

  if (filterProduct && productDomains[filterProduct]) {
    var domainNames = new Set(productDomains[filterProduct]);
    domainNames.forEach(function(d) { activeClasses.add(d); });
    allObjProps.forEach(function(r) {
      if (domainNames.has(r.from) || domainNames.has(r.to)) {
        activeClasses.add(r.from);
        activeClasses.add(r.to);
      }
    });
    activeRels = allObjProps.filter(function(r) { return activeClasses.has(r.from) && activeClasses.has(r.to); });
  } else {
    allObjProps.forEach(function(r) {
      activeClasses.add(r.from);
      activeClasses.add(r.to);
    });
  }

  var lines = ['erDiagram'];
  Array.from(activeClasses).sort().forEach(function(name) {
    var c = allClasses[name];
    if (!c) return;
    lines.push('    ' + name + ' {');
    var attrs = c.attrs.length > 0 ? c.attrs.slice(0, 6) : [{ type: 'string', name: 'label' }];
    attrs.forEach(function(a) { lines.push('        ' + a.type + ' ' + a.name); });
    lines.push('    }');
  });

  activeRels.forEach(function(r) {
    if (r.label === 'inherits') {
      lines.push('    ' + r.from + ' ||--o{ ' + r.to + ' : "extends"');
    } else {
      lines.push('    ' + r.from + ' }o--|| ' + r.to + ' : "' + r.label + '"');
    }
  });

  return { text: lines.join('\n'), entityCount: activeClasses.size, relCount: activeRels.length };
}

function render(filterProduct) {
  var result = buildMermaid(filterProduct);
  var label = filterProduct || 'All Products';
  document.getElementById('meta').textContent =
    result.entityCount + ' entities, ' + result.relCount + ' relationships. ' + label + '. Generated from urn:chorus:ontology via SPARQL.';

  var container = document.getElementById('diagram');
  var id = 'er-' + Date.now();
  return mermaid.render(id, result.text).then(function(rendered) {
    container.innerHTML = rendered.svg;
  }).catch(function(e) {
    container.innerHTML = '<div class="loading">Mermaid render error: ' + e.message + '</div><pre style="font-size:0.75em;max-height:400px;overflow:auto">' + result.text + '</pre>';
    console.error('Mermaid render error:', e);
  });
}

document.getElementById('productFilter').addEventListener('change', function(e) {
  render(e.target.value);
});

loadOntology().then(function() {
  return render('');
}).catch(function(e) {
  document.getElementById('diagram').innerHTML = '<div class="loading">Failed to load: ' + e.message + '</div>';
  console.error('ER diagram load error:', e);
});
