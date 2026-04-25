var N=[], E=[];

// Fetch live data from Fuseki SPARQL endpoint
(async function loadFromFuseki() {
  const FUSEKI = 'http://localhost:3030/pods/sparql';
  const GRAPH = 'urn:chorus:ontology';
  const PREFIX = 'https://jeffbridwell.com/chorus#';
  const FW_PREFIX = 'https://jeffbridwell.com/framework#';
  const JB_PREFIX = 'https://jeffbridwell.com/ontology#';

  async function sparql(query) {
    const resp = await fetch(FUSEKI, {
      method: 'POST',
      headers: { 'Content-Type': 'application/sparql-query', 'Accept': 'application/sparql-results+json' },
      body: query
    });
    const data = await resp.json();
    return data.results.bindings;
  }

  const nodeBindings = await sparql(`
    PREFIX chorus: <${PREFIX}>
    PREFIX fw: <${FW_PREFIX}>
    PREFIX jb: <${JB_PREFIX}>
    PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
    SELECT ?uri ?type ?label ?desc WHERE {
      GRAPH <${GRAPH}> {
        ?uri a ?type .
        FILTER(STRSTARTS(STR(?type), "${PREFIX}") || STRSTARTS(STR(?type), "${FW_PREFIX}") || STRSTARTS(STR(?type), "${JB_PREFIX}"))
        OPTIONAL { ?uri rdfs:label ?label }
        OPTIONAL { ?uri rdfs:comment ?desc }
      }
    }`);

  N = nodeBindings.map(b => ({
    id: b.uri.value.replace(PREFIX, '').replace(FW_PREFIX, '').replace(JB_PREFIX, ''),
    label: b.label ? b.label.value : b.uri.value.replace(PREFIX, '').replace(FW_PREFIX, '').replace(JB_PREFIX, ''),
    cls: b.type.value.replace(PREFIX, '').replace(FW_PREFIX, '').replace(JB_PREFIX, ''),
    desc: b.desc ? b.desc.value.substring(0, 120) : ''
  }));

  const edgeBindings = await sparql(`
    PREFIX chorus: <${PREFIX}>
    PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
    PREFIX owl: <http://www.w3.org/2002/07/owl#>
    PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
    SELECT ?s ?sLabel ?p ?t ?tLabel WHERE {
      GRAPH <${GRAPH}> {
        ?s ?p ?t .
        ?s a ?sType . ?t a ?tType .
        FILTER(STRSTARTS(STR(?p), "${PREFIX}") || STRSTARTS(STR(?p), "${FW_PREFIX}"))
        FILTER(?p != rdf:type)
        FILTER(STRSTARTS(STR(?sType), "${PREFIX}") || STRSTARTS(STR(?sType), "${FW_PREFIX}") || STRSTARTS(STR(?sType), "${JB_PREFIX}"))
        FILTER(STRSTARTS(STR(?tType), "${PREFIX}") || STRSTARTS(STR(?tType), "${FW_PREFIX}") || STRSTARTS(STR(?tType), "${JB_PREFIX}"))
        OPTIONAL { ?s rdfs:label ?sLabel }
        OPTIONAL { ?t rdfs:label ?tLabel }
      }
    }`);

  E = edgeBindings.map(b => ({
    s: b.s.value.replace(PREFIX, '').replace(FW_PREFIX, '').replace(JB_PREFIX, ''),
    t: b.t.value.replace(PREFIX, '').replace(FW_PREFIX, '').replace(JB_PREFIX, ''),
    l: b.p.value.replace(PREFIX, '').replace(FW_PREFIX, ''),
    c: ''
  }));

  document.querySelector('#H p').textContent = N.length + ' instances, ' + E.length + ' edges | Live from Fuseki | Drag | Filter | Hover';
  init();
})().catch(err => {
  console.error('Fuseki fetch failed:', err);
  document.querySelector('#H p').textContent = 'Failed to load from Fuseki — is it running?';
});

var C={"Product": "#4f46e5", "ValueStream": "#22d3ee", "Stage": "#818cf8", "Vertebra": "#818cf8", "GatheringDomain": "#00ff88", "ChorusDomain": "#6366f1", "Service": "#f59e0b", "Tool": "#9ca3af", "Deployment": "#78716c", "Practice": "#c084fc", "Role": "#fbbf24", "HumanRole": "#fbbf24", "AgentRole": "#fbbf24", "Constraint": "#f87171", "Responsibility": "#60a5fa", "Decision": "#fde68a", "Machine": "#06b6d4", "DataSource": "#06b6d4", "InteractionPattern": "#fb923c", "Sequence": "#818cf8", "Story": "#4ade80", "SubProduct": "#4f46e5", "LoomDomain": "#eab308", "SubDomain": "#eab308", "ArtifactType": "#78716c", "ToolType": "#9ca3af", "Gate": "#ef4444", "GateResult": "#fca5a5", "TrustMetric": "#a78bfa", "HandoffType": "#d1d5db", "HandoffStatus": "#d1d5db", "InteractionMode": "#fb923c", "EventCategory": "#94a3b8", "Domain": "#00ff88", "Service": "#f59e0b", "API": "#fb923c", "Skill": "#c084fc"};
var LC={"dependsOn": "#f87171", "hasConstraint": "#f87171", "ownsDomain": "#fbbf24", "ownedBy": "#fbbf24", "ownedByRole": "#fbbf24", "createdBy": "#fbbf24", "participatesIn": "#fbbf24", "affects": "#fde68a", "constrains": "#fde68a", "storyInforms": "#fde68a", "storyConnectsTo": "#4ade80", "triggersSkill": "#fb923c", "detectedIn": "#fb923c", "runsOn": "#06b6d4", "readsFrom": "#06b6d4", "writesTo": "#06b6d4", "touchesDomain": "#818cf8", "belongsToProduct": "#4f46e5", "hasProduct": "#4f46e5", "hasValueStream": "#22d3ee", "servesValueStream": "#22d3ee", "hasPhase": "#818cf8", "belongsTo": "#818cf8", "primaryPhaseProduct": "#c084fc", "nextPhase": "#818cf8", "operatesIn": "#f59e0b", "supportsStream": "#f59e0b", "hasDomain": "#4f46e5", "consumes": "#f87171", "provides": "#22c55e", "primaryStep": "#818cf8", "operatesAt": "#fbbf24", "feeds": "#9ca3af", "nextStage": "#818cf8", "previousStage": "#818cf8", "feedsInto": "#9d174d", "fromStage": "#ef4444", "toStage": "#ef4444", "gatekeeper": "#fbbf24", "outputOf": "#4f46e5", "inputTo": "#4f46e5", "indexes": "#a78bfa", "decomposes": "#fb923c", "servesDomain": "#f59e0b", "containsClass": "#00ff88", "protectedBy": "#ef4444", "ownedBy": "#fbbf24"};
var W=innerWidth,H=innerHeight,M={},vis={},btns={};

function init(){
N.forEach(function(n,i){
  M[n.id]=n;
  var a=i*6.28/N.length;n.x=W/2+Math.cos(a)*300;n.y=H/2+Math.sin(a)*300;
});

// === DISPLAY CATEGORIES ===
// Map OWL types to Jeff's display categories
var DC={"Product":"Product","SubProduct":"Sub-Product","SubDomain":"Sub-Domain","Domain":"Domain","CollectionDomain":"Domain","Vertebra":"Step","ValueStream":"Value Stream","HumanRole":"Owner","AgentRole":"Owner","Machine":"Infra","Role":"Owner"};
var catTypes={};
var catColors={"Product":"#4f46e5","Sub-Product":"#4f46e5","Sub-Domain":"#eab308","Domain":"#00ff88","Step":"#818cf8","Value Stream":"#22d3ee","Owner":"#fbbf24","Infra":"#06b6d4"};

var cls=[];var ss={};
N.forEach(function(n){if(!ss[n.cls]){ss[n.cls]=1;cls.push(n.cls)}});
cls.forEach(function(c){var cat=DC[c]||"Other";if(!catTypes[cat])catTypes[cat]=[];catTypes[cat].push(c)});

var defaultCats={"Product":1,"Sub-Product":1,"Domain":1,"Sub-Domain":1,"Value Stream":1,"Step":1,"Owner":1,"Infra":1};
cls.forEach(function(c){var cat=DC[c]||"Other";vis[c]=!!defaultCats[cat]});

var cats=Object.keys(catTypes).filter(function(c){return c!=="Other"}).sort();

// === UNIFIED NODE VISIBILITY ===
// Single source of truth: is this specific node checked in its dropdown?
var nodeVis={};
N.forEach(function(n){nodeVis[n.id]=true});

// === PRODUCT MEMBERSHIP ===
// Build exclusive product membership. Each node belongs to exactly one product (or "shared").
// Containment edges: hasDomain, hasSubProduct (parent→child), belongsTo (child→parent for streams/steps)
var productFilter={gathering:true,chorus:true,borg:true};
var nodeProd={}; // nodeId → 'gathering'|'chorus'|'borg'|null

// Walk downward from product roots via containment
function assignProduct(root, product) {
  if(nodeProd[root] && nodeProd[root] !== product) {
    nodeProd[root] = 'shared'; return; // claimed by multiple products
  }
  if(nodeProd[root] === product) return; // already assigned
  nodeProd[root] = product;
  E.forEach(function(e){
    if(e.s===root && (e.l==='hasDomain'||e.l==='hasSubProduct')) assignProduct(e.t, product);
  });
  // Walk belongsTo in reverse (children pointing to this node)
  E.forEach(function(e){
    if(e.t===root && e.l==='belongsTo') assignProduct(e.s, product);
  });
}
assignProduct('gathering', 'gathering');
assignProduct('chorusProduct', 'chorus');
assignProduct('borgProduct', 'borg');

// Steps: assign via primaryStep edges (domain→step). Must run BEFORE value streams.
E.forEach(function(e){
  if(e.l==='primaryStep' && nodeProd[e.s] && nodeProd[e.s] !== 'shared') {
    var domProd = nodeProd[e.s];
    if(!nodeProd[e.t]) nodeProd[e.t] = domProd;
    else if(nodeProd[e.t] !== domProd && nodeProd[e.t] !== 'shared') nodeProd[e.t] = 'shared';
  }
});

// Value streams: assign based on which steps belong to them.
// Steps now have product assignments from primaryStep pass above.
E.forEach(function(e){
  if(e.l==='belongsTo' && M[e.t] && M[e.t].cls==='ValueStream') {
    var stepProd = nodeProd[e.s];
    if(stepProd && stepProd !== 'shared') {
      if(!nodeProd[e.t]) nodeProd[e.t] = stepProd;
      else if(nodeProd[e.t] !== stepProd) nodeProd[e.t] = 'shared';
    }
  }
});

// Back-propagate: steps that belongsTo a value stream inherit the stream's product
E.forEach(function(e){
  if(e.l==='belongsTo' && M[e.t] && M[e.t].cls==='ValueStream' && nodeProd[e.t]) {
    var streamProd = nodeProd[e.t];
    if(!nodeProd[e.s]) nodeProd[e.s] = streamProd;
    else if(nodeProd[e.s] !== streamProd && nodeProd[e.s] !== 'shared') nodeProd[e.s] = 'shared';
  }
});

// === CATEGORY DROPDOWN BUTTONS ===
cats.forEach(function(cat){
  var wrap=document.createElement("div");
  wrap.className="b";
  wrap.style.borderColor=catColors[cat]||"#888";
  btns[cat]=wrap;

  var items=[];
  catTypes[cat].forEach(function(t){N.forEach(function(n){if(n.cls===t)items.push(n)})});
  items.sort(function(a,b){return(a.label||a.id).localeCompare(b.label||b.id)});
  var count=items.length;

  var hdr=document.createElement("span");
  hdr.style.cursor="pointer";
  wrap.appendChild(hdr);

  var arrow=document.createElement("span");
  arrow.textContent=" \u25BE";
  arrow.style.cursor="pointer";
  arrow.style.fontSize="0.6rem";
  wrap.appendChild(arrow);

  var dd=document.createElement("div");
  dd.className="dd";

  var allLabel=document.createElement("label");
  allLabel.className="dd-all";
  var allCb=document.createElement("input");
  allCb.type="checkbox";allCb.checked=true;
  allLabel.appendChild(allCb);
  allLabel.appendChild(document.createTextNode("All "+cat));
  dd.appendChild(allLabel);

  var cbs=[];
  items.forEach(function(item){
    var lbl=document.createElement("label");
    var cb=document.createElement("input");
    cb.type="checkbox";cb.checked=true;
    cb.dataset.nodeId=item.id;
    cb.onchange=function(){
      nodeVis[item.id]=cb.checked;
      var allOn=cbs.every(function(c){return c.checked});
      var someOn=cbs.some(function(c){return c.checked});
      allCb.checked=allOn;allCb.indeterminate=!allOn&&someOn;
      syncVisFromNodeVis();
      updateCatBtn();render();saveState();
    };
    cbs.push(cb);
    lbl.appendChild(cb);
    lbl.appendChild(document.createTextNode(item.label||item.id));
    dd.appendChild(lbl);
  });

  allCb.onchange=function(){
    cbs.forEach(function(cb){cb.checked=allCb.checked;nodeVis[cb.dataset.nodeId]=allCb.checked});
    allCb.indeterminate=false;
    syncVisFromNodeVis();
    updateCatBtn();render();saveState();
  };

  wrap.appendChild(dd);

  function syncVisFromNodeVis(){
    // vis[type] is ON if ANY node of that type is checked
    catTypes[cat].forEach(function(t){
      var anyChecked=false;
      items.forEach(function(item){if(M[item.id]&&M[item.id].cls===t&&nodeVis[item.id]!==false)anyChecked=true});
      vis[t]=anyChecked;
    });
  }

  function updateCatBtn(){
    var onCount=items.filter(function(item){return nodeVis[item.id]!==false}).length;
    var anyOn=onCount>0;
    hdr.textContent=cat+"("+onCount+"/"+count+")";
    wrap.style.backgroundColor=anyOn?(catColors[cat]||"#888"):"transparent";
    wrap.style.color=anyOn?"#fff":"#666";
    wrap.style.borderColor=anyOn?(catColors[cat]||"#888"):"#444";
  }
  updateCatBtn();

  // Header click: toggle all items in category
  hdr.onclick=function(e){
    e.stopPropagation();
    var anyOn=items.some(function(item){return nodeVis[item.id]!==false});
    items.forEach(function(item){nodeVis[item.id]=!anyOn});
    cbs.forEach(function(cb){cb.checked=!anyOn});
    allCb.checked=!anyOn;allCb.indeterminate=false;
    syncVisFromNodeVis();
    updateCatBtn();render();saveState();
  };

  // Arrow click: toggle dropdown
  arrow.onclick=function(e){
    e.stopPropagation();
    document.querySelectorAll('.dd.open').forEach(function(d){if(d!==dd)d.classList.remove('open')});
    dd.classList.toggle('open');
  };

  document.getElementById("F").appendChild(wrap);
});

// Close dropdowns on outside click
document.addEventListener('click',function(e){
  if(!e.target.closest('.b'))document.querySelectorAll('.dd.open').forEach(function(d){d.classList.remove('open')});
});

// Product filter driven by Product dropdown checkboxes — no separate buttons.
// Map product node IDs to productFilter keys.
var productNodeMap={'gathering':'gathering','chorusProduct':'chorus','borgProduct':'borg'};

// Reset button
var rb=document.createElement("div");
rb.className="b";rb.style.color="#f87171";rb.style.borderColor="#f87171";rb.textContent="Reset";
rb.onclick=function(){localStorage.removeItem('chorus-owl-pos');localStorage.removeItem('chorus-owl-flt');localStorage.removeItem('chorus-owl-nodevis');location.reload()};
document.getElementById("F").appendChild(rb);

// === STATE PERSISTENCE ===
try{
  var sp=JSON.parse(localStorage.getItem('chorus-owl-pos')||'{}');
  N.forEach(function(n){if(sp[n.id]){n.x=sp[n.id].x;n.y=sp[n.id].y;n.fx=sp[n.id].x;n.fy=sp[n.id].y;}});
  // Restore nodeVis (single source of truth for filters)
  var snv=JSON.parse(localStorage.getItem('chorus-owl-nodevis')||'null');
  if(snv){
    Object.keys(snv).forEach(function(k){
      if(M[k]) nodeVis[k]=snv[k];
    });
    // Sync vis and checkbox UI from restored nodeVis
    cats.forEach(function(cat){
      catTypes[cat].forEach(function(t){
        var anyChecked=false;
        N.forEach(function(n){if(n.cls===t&&nodeVis[n.id]!==false)anyChecked=true});
        vis[t]=anyChecked;
      });
    });
  }
}catch(e){/* ignore */}

function saveState(){
  var p={};N.forEach(function(n){if(n.x&&n.y)p[n.id]={x:n.x,y:n.y}});
  localStorage.setItem('chorus-owl-pos',JSON.stringify(p));
  // Save nodeVis as single source of truth (replaces old chorus-owl-flt)
  var nv={};Object.keys(nodeVis).forEach(function(k){if(nodeVis[k]===false)nv[k]=false});
  localStorage.setItem('chorus-owl-nodevis',JSON.stringify(nv));
}
setInterval(saveState,5000);

// === D3 FORCE LAYOUT ===
var svg=d3.select("#G");
var tt=document.getElementById("T");
var g=svg.append("g");var gL=g.append("g"),gN=g.append("g"),gT=g.append("g");
svg.call(d3.zoom().scaleExtent([0.2,5]).on("zoom",function(e){g.attr("transform",e.transform)}));
var sim=d3.forceSimulation()
  .force("charge",d3.forceManyBody().strength(-120))
  .force("cx",d3.forceX(W/2).strength(0.04))
  .force("cy",d3.forceY(H/2).strength(0.04))
  .force("coll",d3.forceCollide(function(d){return sz(d.cls)+4}))
  .on("tick",tick);

function sz(c){
  if(c==="Product")return 18;if(c==="Role"||c==="HumanRole"||c==="AgentRole")return 16;if(c==="ValueStream")return 15;
  if(c==="SubProduct")return 14;if(c==="Machine")return 13;
  if(c==="Service"||c==="InfrastructureService"||c==="ProtocolService")return 12;
  if(c==="GatheringDomain"||c==="ChorusDomain"||c==="Domain"||c==="TechnicalDomain")return 13;
  if(c==="LoomDomain")return 11;if(c==="Skill")return 8;
  if(c==="Stage"||c==="Vertebra")return 10;return 7;
}
function fs(c){
  if(c==="Product"||c==="Role"||c==="HumanRole"||c==="AgentRole")return"11px";
  if(c==="SubProduct"||c==="ValueStream")return"9px";
  if(c==="Service"||c==="InfrastructureService"||c==="ProtocolService"||c==="GatheringDomain"||c==="ChorusDomain"||c==="Domain"||c==="TechnicalDomain"||c==="Machine")return"8px";
  if(c==="LoomDomain")return"7.5px";return"7px";
}

// === CONTAINMENT EDGES — parent→child relationships ===
// Used for cascade filtering: hiding a parent hides its children.
var containmentEdges=['hasDomain','hasSubProduct','hasValueStream'];
var childToParents={};  // childId → [parentId, ...]
var parentToChildren={}; // parentId → [childId, ...]
E.forEach(function(e){
  // Parent→child: hasDomain, hasSubProduct, hasValueStream
  if(containmentEdges.indexOf(e.l)>=0){
    if(!parentToChildren[e.s])parentToChildren[e.s]=[];
    parentToChildren[e.s].push(e.t);
    if(!childToParents[e.t])childToParents[e.t]=[];
    childToParents[e.t].push(e.s);
  }
  // Child→parent: belongsTo (reverse containment)
  if(e.l==='belongsTo'){
    if(!childToParents[e.s])childToParents[e.s]=[];
    childToParents[e.s].push(e.t);
    if(!parentToChildren[e.t])parentToChildren[e.t]=[];
    parentToChildren[e.t].push(e.s);
  }
});

// === RENDER — SINGLE VISIBILITY FUNCTION ===
// Visibility rules (AND):
// 1. Type category on (vis[cls])
// 2. Individual checkbox checked (nodeVis[id])
// 3. Product active OR no product assignment
// 4. CASCADE: at least one containment parent is visible (or node has no parents)
function render(){
  // Sync productFilter from Product dropdown state
  Object.keys(productNodeMap).forEach(function(nodeId){
    productFilter[productNodeMap[nodeId]]=nodeVis[nodeId]!==false;
  });

  // Pass 1: basic visibility (type + checkbox + product)
  var basicVis={};
  N.forEach(function(n){
    if(!vis[n.cls]){basicVis[n.id]=false;return}
    if(nodeVis[n.id]===false){basicVis[n.id]=false;return}
    var p=nodeProd[n.id];
    if(!p){basicVis[n.id]=true;return}
    if(p==='shared'){
      var prods={};
      E.forEach(function(e){
        if(e.s===n.id&&nodeProd[e.t])prods[nodeProd[e.t]]=1;
        if(e.t===n.id&&nodeProd[e.s])prods[nodeProd[e.s]]=1;
      });
      basicVis[n.id]=!!(
        (prods.gathering&&productFilter.gathering)||
        (prods.chorus&&productFilter.chorus)||
        (prods.borg&&productFilter.borg)||
        Object.keys(prods).length===0
      );
      return;
    }
    basicVis[n.id]=productFilter[p]||false;
  });

  // Pass 2: cascade — hide nodes whose ALL containment parents are hidden.
  // Repeat until stable (handles multi-level nesting).
  var changed=true;
  while(changed){
    changed=false;
    N.forEach(function(n){
      if(!basicVis[n.id])return; // already hidden
      var parents=childToParents[n.id];
      if(!parents||parents.length===0)return; // root node, no cascade
      // If ALL parents are hidden, hide this node too
      var anyParentVisible=false;
      parents.forEach(function(pid){if(basicVis[pid])anyParentVisible=true});
      if(!anyParentVisible){basicVis[n.id]=false;changed=true}
    });
  }

  var vn=N.filter(function(n){return basicVis[n.id]});
  var vi={};vn.forEach(function(n){vi[n.id]=1});
  var ve=[];
  E.forEach(function(l){if(vi[l.s]&&vi[l.t]&&M[l.s]&&M[l.t])ve.push({source:M[l.s],target:M[l.t],c:LC[l.l]||l.c||"#444"})});
  var lw=vn.length<30?2:vn.length<80?1:0.5;
  var lo=vn.length<30?0.7:vn.length<80?0.4:0.25;
  var le=gL.selectAll("line").data(ve);le.exit().remove();
  le=le.enter().append("line").merge(le);
  le.attr("stroke",function(d){return d.c}).attr("stroke-width",lw).attr("stroke-opacity",lo);
  var ne=gN.selectAll("circle").data(vn,function(d){return d.id});ne.exit().remove();
  ne=ne.enter().append("circle").attr("cursor","grab")
    .on("mouseover",function(e,d){tt.style.display="block";tt.innerHTML="<b style='color:"+(C[d.cls]||"#888")+"'>"+(d.label||d.id)+"</b><br><span style='color:#666;font-size:0.6rem'>"+(DC[d.cls]||d.cls)+"</span>"+(nodeProd[d.id]?" <span style='color:#999;font-size:0.55rem'>["+nodeProd[d.id]+"]</span>":"")+"<br><span style='color:#4f46e5;font-size:0.65rem'>"+(d.desc||"")+"</span>"})
    .on("mousemove",function(e){tt.style.left=(e.clientX+12)+"px";tt.style.top=(e.clientY-8)+"px"})
    .on("mouseout",function(){tt.style.display="none"})
    .call(d3.drag()
      .on("start",function(e,d){sim.alphaTarget(0.1).restart();d.fx=d.x;d.fy=d.y})
      .on("drag",function(e,d){d.fx=e.x;d.fy=e.y})
      .on("end",function(e,d){sim.alphaTarget(0);d.fx=d.x;d.fy=d.y;saveState()})
    ).merge(ne);
  ne.attr("r",function(d){return sz(d.cls)}).attr("fill",function(d){return C[d.cls]||"#888"}).attr("fill-opacity",0.85).attr("stroke",function(d){return C[d.cls]||"#888"}).attr("stroke-width",1).attr("stroke-opacity",0.3);
  var te=gT.selectAll("text").data(vn,function(d){return d.id});te.exit().remove();
  te=te.enter().append("text").attr("pointer-events","none").attr("text-anchor","middle").attr("fill","#ccc").merge(te);
  te.attr("font-size",function(d){return fs(d.cls)}).attr("dy",function(d){return sz(d.cls)+10}).text(function(d){return d.label||d.id});
  sim.nodes(vn);
  sim.force("link",d3.forceLink(ve).id(function(d){return d.id}).distance(function(){return vn.length<30?120:vn.length<80?70:50}).strength(0.4));
  sim.alpha(0.5).restart();
}
render();

function tick(){
  gL.selectAll("line").attr("x1",function(d){return d.source.x}).attr("y1",function(d){return d.source.y}).attr("x2",function(d){return d.target.x}).attr("y2",function(d){return d.target.y});
  gN.selectAll("circle").attr("cx",function(d){return d.x}).attr("cy",function(d){return d.y});
  gT.selectAll("text").attr("x",function(d){return d.x}).attr("y",function(d){return d.y});
}
} // end init
