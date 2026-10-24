(function () {
  "use strict";
  var D = JSON.parse(document.getElementById("ig-data").textContent);
  var byId = {};
  D.nodes.forEach(function (n) { byId[n.id] = n; });
  var findingsByNode = {};
  D.findings.forEach(function (f) {
    if (f.pinned) (findingsByNode[f.pinned] = findingsByNode[f.pinned] || []).push(f);
  });

  function el(tag, attrs, children) {
    var e = document.createElement(tag);
    Object.keys(attrs || {}).forEach(function (k) {
      if (k === "text") e.textContent = attrs[k];
      else if (k === "class") e.className = attrs[k];
      else if (k.slice(0, 2) === "on") e.addEventListener(k.slice(2), attrs[k]);
      else e.setAttribute(k, attrs[k]);
    });
    (children || []).forEach(function (c) { if (c) e.appendChild(typeof c === "string" ? document.createTextNode(c) : c); });
    return e;
  }
  function css(name) { return getComputedStyle(document.documentElement).getPropertyValue(name).trim(); }
  function base(p) { return p.split("/").pop(); }

  // Editor links: URL schemes become links; javascript:/data:/vbscript: never do.
  function editorLink(file, line) {
    var abs = D.root.replace(/\/$/, "") + "/" + file;
    var target = D.editor.split("{file}").join(abs).split("{line}").join(String(line || 1));
    var m = /^([a-z][a-z0-9+.-]*):/i.exec(target);
    if (m && ["javascript", "data", "vbscript"].indexOf(m[1].toLowerCase()) < 0) {
      return el("a", { href: target, text: file + ":" + line });
    }
    return el("code", { text: file + ":" + line, title: target });
  }

  // ---- Modules, members and the drill-down tree (layer → directories → module → members) ----
  var modules = {};
  function moduleFor(file) {
    if (!modules[file]) {
      var n = byId[file] || { id: file, name: base(file), file: file, kind: "module", status: "unchanged", module: true, intents: [], layer: null, line: 1, endLine: 1, complexity: 1, loc: 0 };
      byId[file] = byId[file] || n;
      modules[file] = { node: n, members: [], dirty: false, findings: 0 };
    }
    return modules[file];
  }
  D.nodes.forEach(function (n) {
    var m = moduleFor(n.file);
    if (!n.module) m.members.push(n);
    if (n.status !== "unchanged") m.dirty = true;
    m.findings += (findingsByNode[n.id] || []).length;
  });
  // The visible member for a node is its top-level ancestor inside the module.
  function topMember(n) {
    var cur = n;
    while (cur.parent && byId[cur.parent] && !byId[cur.parent].module) cur = byId[cur.parent];
    return cur;
  }

  var root = { id: "g:", label: "", children: {}, mods: [], parent: null };
  var treeOf = {};
  function child(t, key, label) {
    if (!t.children[key]) t.children[key] = { id: t.id + "/" + key, label: label, children: {}, mods: [], parent: t };
    return t.children[key];
  }
  var byLayer = {};
  Object.keys(modules).sort().forEach(function (file) {
    var l = modules[file].node.layer;
    (byLayer[l || ""] = byLayer[l || ""] || []).push(file);
  });
  Object.keys(byLayer).forEach(function (l) {
    var files = byLayer[l];
    var top = root, prefix = [];
    if (l) {
      top = child(root, "L:" + l, l);
      top.layer = true;
      // Directories are relative to the deepest directory all of the layer's files share.
      prefix = files[0].split("/").slice(0, -1);
      files.forEach(function (f) {
        var d = f.split("/").slice(0, -1), i = 0;
        while (i < prefix.length && i < d.length && prefix[i] === d[i]) i++;
        prefix = prefix.slice(0, i);
      });
    }
    files.forEach(function (f) {
      var dirs = f.split("/").slice(prefix.length, -1), t = top;
      dirs.forEach(function (d) { t = child(t, d, d); });
      t.mods.push(f);
      treeOf[f] = t;
    });
  });
  // Merge single-child directory chains ("a" → "b" → "c" becomes "a/b/c").
  (function compress(t) {
    Object.keys(t.children).forEach(function (k) {
      var c = t.children[k];
      while (Object.keys(c.children).length === 1 && c.mods.length === 0) {
        var g = c.children[Object.keys(c.children)[0]];
        if (c.layer) { c.children = g.children; c.mods = g.mods; Object.keys(g.children).forEach(function (x) { g.children[x].parent = c; }); g.mods.forEach(function (m) { treeOf[m] = c; }); continue; }
        g.label = c.label + "/" + g.label; g.parent = t; t.children[k] = g; c = g;
      }
      compress(c);
    });
  })(root);
  // A directory holding a single module and nothing else is shown as that module.
  (function hoist(t) {
    Object.keys(t.children).forEach(function (k) {
      var c = t.children[k];
      hoist(c);
      if (!c.layer && Object.keys(c.children).length === 0 && c.mods.length === 1) {
        t.mods.push(c.mods[0]); treeOf[c.mods[0]] = t; delete t.children[k];
      }
    });
  })(root);
  var groups = {};
  (function stats(t) {
    groups[t.id] = t;
    t.count = t.mods.length; t.dirty = false; t.findings = 0;
    t.mods.forEach(function (m) { t.dirty = t.dirty || modules[m].dirty; t.findings += modules[m].findings; });
    Object.keys(t.children).forEach(function (k) {
      var c = t.children[k];
      stats(c);
      t.count += c.count; t.dirty = t.dirty || c.dirty; t.findings += c.findings;
    });
  })(root);
  function ancestors(t) { var a = []; while (t && t !== root) { a.unshift(t); t = t.parent; } return a; }

  function nodeData(n, parent) {
    return { id: n.id, label: n.name, kind: n.module ? "module" : "member", status: n.status,
             findings: (findingsByNode[n.id] || []).length, parent: parent };
  }
  function aggregateEdges(repOf, keep) {
    var agg = {}, out = [];
    D.edges.forEach(function (e) {
      var a = repOf(e.from), b = repOf(e.to);
      if (!a || !b || a === b || !keep(a) || !keep(b)) return;
      var k = a + "|" + b, x = agg[k];
      if (!x) { x = agg[k] = { source: a, target: b, count: 0, violation: false, status: "unchanged", guessed: true }; out.push(x); }
      x.count++;
      x.violation = x.violation || e.violation;
      x.guessed = x.guessed && e.confidence === "guessed";
      if (e.status !== "unchanged" && x.status === "unchanged") x.status = e.status;
    });
    return out;
  }

  // Architecture is flat: the children of the current scope, plus — dimmed, as context — the siblings
  // at every level above it that connect to the scope. Opening a directory makes it the scope.
  var scope = root;
  function rep(id) {
    var n = byId[id]; if (!n) return null;
    var path = ancestors(treeOf[n.file]), chain = ancestors(scope);
    for (var i = 0; i < chain.length; i++) {
      if (i >= path.length) return n.file;
      if (path[i] !== chain[i]) return path[i].id;
    }
    return path.length > chain.length ? path[chain.length].id : n.file;
  }
  function insideScope(repId) {
    var g = groups[repId];
    if (g) return g.parent === scope;
    return treeOf[repId] === scope;
  }

  function architecture() {
    var els = [], shown = {}, inner = {};
    Object.keys(modules).forEach(function (f) { var r = rep(f); if (insideScope(r)) inner[r] = true; });
    var edges = aggregateEdges(rep, function () { return true; }).filter(function (e) { return inner[e.source] || inner[e.target]; });
    var linked = {};
    edges.forEach(function (e) { linked[e.source] = linked[e.target] = true; });
    function add(id, outer) {
      if (shown[id]) return;
      shown[id] = true;
      var g = groups[id];
      if (g) {
        els.push({ group: "nodes", data: { id: id, label: g.label + " · " + g.count, kind: g.layer ? "layer" : "dir", count: g.count,
          status: g.dirty ? "changed" : "unchanged", findings: g.findings, outer: outer } });
      } else {
        var m = modules[id], d = nodeData(m.node);
        d.status = m.node.status !== "unchanged" ? m.node.status : (m.dirty ? "changed" : "unchanged");
        d.findings = m.findings; d.outer = outer;
        els.push({ group: "nodes", data: d });
      }
    }
    Object.keys(inner).forEach(function (id) { add(id, false); });
    Object.keys(linked).forEach(function (id) { add(id, true); });
    edges.forEach(function (e, i) {
      if (shown[e.source] && shown[e.target]) { e.id = "e" + i; e.ctx = !(inner[e.source] && inner[e.target]); els.push({ group: "edges", data: e }); }
    });
    return els;
  }

  function setScope(t) {
    scope = t || root;
    mode = "architecture";
    render();
    crumbs();
  }
  var crumbBar = document.getElementById("crumbs");
  function crumbs() {
    crumbBar.innerHTML = "";
    if (mode !== "architecture") return;
    [root].concat(ancestors(scope)).forEach(function (t, i) {
      if (i) crumbBar.appendChild(document.createTextNode(" › "));
      crumbBar.appendChild(t === scope ? el("strong", { text: i ? t.label : "All" }) :
        el("a", { href: "#", text: i ? t.label : "All", onclick: function (e) { e.preventDefault(); setScope(t); groupDetails(t.id); } }));
    });
  }

  // Exact nodes (ids), boxed in their modules.
  function exact(ids) {
    var els = [], shown = {};
    Object.keys(ids).forEach(function (id) {
      var n = byId[id]; if (!n) return;
      if (n.module) return;
      if (!shown[n.file]) { var m = byId[n.file]; els.push({ group: "nodes", data: { id: n.file, label: base(n.file), kind: "openmodule", status: m.status } }); shown[n.file] = true; }
      els.push({ group: "nodes", data: nodeData(n, n.file) }); shown[id] = true;
    });
    Object.keys(ids).forEach(function (id) {
      var n = byId[id];
      if (n && n.module && !shown[id]) { els.push({ group: "nodes", data: nodeData(n) }); shown[id] = true; }
    });
    aggregateEdges(function (x) { return x; }, function (x) { return shown[x]; }).forEach(function (e, i) { e.id = "e" + i; els.push({ group: "edges", data: e }); });
    return els;
  }

  function changedIds() {
    var ids = {}, changed = {};
    D.nodes.forEach(function (n) { if (n.status !== "unchanged") changed[n.id] = ids[n.id] = true; });
    D.edges.forEach(function (e) { if (changed[e.from]) ids[e.to] = true; if (changed[e.to]) ids[e.from] = true; });
    return ids;
  }

  var focusId = null, focusDepth = 1;
  function focusIds() {
    var ids = {}; ids[focusId] = true;
    var frontier = [focusId];
    for (var d = 0; d < focusDepth; d++) {
      var next = [];
      D.edges.forEach(function (e) {
        frontier.forEach(function (f) {
          if (e.from === f && !ids[e.to]) { ids[e.to] = true; next.push(e.to); }
          if (e.to === f && !ids[e.from]) { ids[e.from] = true; next.push(e.from); }
        });
      });
      frontier = next;
    }
    // A module in focus brings its members' neighbours too.
    var n = byId[focusId];
    if (n && n.module) modules[focusId].members.forEach(function (m) { if (topMember(m) === m) ids[m.id] = true; });
    return ids;
  }

  // ---- Cytoscape ----
  if (window.cytoscapeFcose) cytoscape.use(window.cytoscapeFcose);
  var color = { added: css("--added"), changed: css("--changed"), removed: css("--removed"), unchanged: css("--unchanged") };
  var fg = css("--fg"), muted = css("--muted"), line = css("--line"), err = css("--error"), accent = css("--accent"), panel = css("--panel");
  function statusColor(n) { return color[n.data("status")] || color.unchanged; }
  var cy = cytoscape({
    container: document.getElementById("cy"),
    minZoom: 0.05,
    maxZoom: 3,
    style: [
      { selector: "node", style: {
        "label": "data(label)", "font-size": 10, "color": fg, "text-valign": "bottom", "text-margin-y": 3,
        "min-zoomed-font-size": 7, "background-color": statusColor, "width": 14, "height": 14,
        "text-wrap": "ellipsis", "text-max-width": 160 } },
      { selector: "node[kind = 'module']", style: { "width": 20, "height": 20, "shape": "round-rectangle" } },
      { selector: "node[kind = 'layer'], node[kind = 'dir']", style: {
        "shape": "round-rectangle", "height": "mapData(count, 1, 300, 34, 80)",
        "width": function (n) { return Math.max(70, n.data("label").length * 8.5 + 24); },
        "text-valign": "center", "text-margin-y": 0, "font-size": 13, "font-weight": 600, "min-zoomed-font-size": 4,
        "background-color": panel, "border-width": 2, "border-color": statusColor, "text-max-width": 400 } },
      { selector: "node[kind = 'layer']", style: { "border-width": 3, "font-size": 15 } },
      { selector: "node[kind = 'open'], node[kind = 'openmodule']", style: {
        "shape": "round-rectangle", "background-color": panel, "background-opacity": 0.5, "border-width": 1.5, "border-style": "dashed",
        "border-color": statusColor, "text-valign": "top", "text-halign": "center", "text-margin-y": -4, "font-size": 12,
        "font-weight": 600, "padding": 14, "min-zoomed-font-size": 5 } },
      { selector: "node[?outer]", style: { "opacity": 0.55, "border-style": "dashed" } },
      { selector: "node[status = 'removed']", style: { "border-style": "dashed", "background-opacity": 0.35 } },
      { selector: "node[findings > 0]", style: { "border-width": 3, "border-color": err, "border-style": "solid" } },
      { selector: "node:selected", style: { "border-width": 4, "border-color": accent, "border-style": "solid" } },
      { selector: "edge", style: {
        "width": "mapData(count, 1, 40, 1, 7)", "curve-style": "bezier", "target-arrow-shape": "triangle", "arrow-scale": 0.8,
        "line-color": muted, "target-arrow-color": muted, "opacity": 0.55,
        "label": "data(count)", "font-size": 9, "color": muted, "min-zoomed-font-size": 9, "text-background-color": panel,
        "text-background-opacity": 1, "text-background-padding": 1 } },
      { selector: "edge[count = 1]", style: { "label": "" } },
      { selector: "edge[status = 'added']", style: { "line-color": color.added, "target-arrow-color": color.added, "opacity": 0.9 } },
      { selector: "edge[status = 'removed']", style: { "line-style": "dashed", "line-color": color.removed, "target-arrow-color": color.removed } },
      { selector: "edge[?guessed]", style: { "line-style": "dotted" } },
      { selector: "edge[?ctx]", style: { "opacity": 0.22 } },
      { selector: "edge[?violation]", style: { "line-color": err, "target-arrow-color": err, "opacity": 1, "width": "mapData(count, 1, 40, 2.5, 8)" } },
      { selector: ".faded", style: { "opacity": 0.12 } }
    ]
  });

  window.igCy = cy; // for debugging from the console
  var mode = D.mode;
  var hint = document.getElementById("hint");
  // Re-renders the current mode. Within a mode, nodes keep their positions and new ones start next to
  // the box they came out of, so opening a box doesn't reshuffle the picture. `zoomTo` fits that box.
  var lastMode = null;
  function render(zoomTo) {
    var els = mode === "architecture" ? architecture() : mode === "focus" ? exact(focusIds()) : exact(changedIds());
    var fresh = mode !== lastMode;
    lastMode = mode;
    var prev = {};
    if (!fresh) cy.nodes().forEach(function (n) { prev[n.id()] = Object.assign({}, n.position()); });
    cy.startBatch();
    cy.elements().remove();
    cy.add(els);
    var byParent = {};
    els.forEach(function (x) { if (x.group === "nodes") byParent[x.data.id] = x.data.parent; });
    cy.nodes().forEach(function (n) {
      var p = prev[n.id()], up = n.id();
      while (!p && up) { up = byParent[up]; p = up && prev[up]; }
      if (p) n.position({ x: p.x + (prev[n.id()] ? 0 : Math.random() * 40 - 20), y: p.y + (prev[n.id()] ? 0 : Math.random() * 40 - 20) });
    });
    cy.endBatch();
    var layout = cy.layout(window.cytoscapeFcose ? {
      name: "fcose", quality: "proof", randomize: true, animate: false, fit: false,
      nodeDimensionsIncludeLabels: true, packComponents: true, nodeRepulsion: function () { return 60000; },
      idealEdgeLength: function () { return 160; }, edgeElasticity: function () { return 0.2; }, nestingFactor: 0.4,
      gravity: 0.15, gravityRange: 5, numIter: 4000, tile: true, tilingPaddingVertical: 20, tilingPaddingHorizontal: 20,
      nodeSeparation: 120, padding: 20
    } : { name: "cose", animate: false, randomize: fresh, fit: false, nodeDimensionsIncludeLabels: true });
    layout.one("layoutstop", function () {
      var target = zoomTo && cy.getElementById(zoomTo);
      if (target && target.length) cy.fit(target.union(target.descendants()).union(target.neighborhood()), 40);
      else cy.fit(undefined, 30);
      if (cy.zoom() > 1.4) cy.zoom({ level: 1.4, renderedPosition: { x: cy.width() / 2, y: cy.height() / 2 } });
    });
    layout.run();
    if (mode === "focus" && focusId) cy.getElementById(focusId).select();
    document.querySelectorAll(".modes button").forEach(function (b) {
      b.setAttribute("aria-pressed", String(b.dataset.mode === mode));
      if (b.dataset.mode === "focus") b.disabled = !focusId;
    });
    var n = cy.nodes().length;
    crumbs();
    hint.textContent = mode === "architecture" ? "Click a box to open it. Dimmed boxes are outside, connected context." :
      mode === "focus" ? "Neighbourhood of " + (byId[focusId] ? byId[focusId].name : "") + " (depth " + focusDepth + ")" :
      (n ? "" : "No changes against the base.");
  }
  document.querySelectorAll(".modes button").forEach(function (b) {
    b.addEventListener("click", function () { if (b.dataset.mode === "focus" && !focusId) return; mode = b.dataset.mode; render(); });
  });

  function focus(id) {
    if (!byId[id]) return;
    focusId = id; mode = "focus"; render(); details(id);
  }

  function byParentOf(id) { var n = cy.getElementById(id); return n.length && n.parent().length ? n.parent().id() : null; }

  cy.on("tap", function (e) {
    var t = e.target;
    if (t === cy) { cy.elements().removeClass("faded"); return; }
    if (!t.isNode()) return;
    var k = t.data("kind"), id = t.id();
    if (mode === "architecture" && (k === "layer" || k === "dir")) { setScope(groups[id]); groupDetails(id); return; }
    details(id);
    var nb = t.closedNeighborhood().union(t.ancestors());
    cy.elements().addClass("faded"); nb.removeClass("faded"); nb.ancestors().removeClass("faded");
  });

  // ---- Side panel ----
  var side = document.getElementById("side");
  function findingItem(f) {
    return el("li", { class: "f", onclick: function () { if (f.pinned) focus(f.pinned); } }, [
      el("span", { class: "sev " + f.severity, text: f.severity }),
      el("span", { text: f.message }),
      el("div", { class: "loc" }, [f.ruleId + " · ", editorLink(f.file, f.line)])
    ]);
  }
  function backLink() {
    return el("p", {}, [el("a", { href: "#", onclick: function (e) { e.preventDefault(); cy.$(":selected").unselect(); cy.elements().removeClass("faded"); overview(); }, text: "← overview" })]);
  }
  function overview() {
    side.innerHTML = "";
    D.notices.forEach(function (n) { side.appendChild(el("p", { class: "notice", text: n })); });
    if (D.intentChanges.length) {
      side.appendChild(el("h2", { text: "Intent changes" }));
      side.appendChild(el("ul", {}, D.intentChanges.map(function (c) {
        var mark = { added: "+", changed: "~", removed: "-" }[c.status];
        return el("li", { class: c.status, text: mark + " " + c.id + ": " + c.text });
      })));
    }
    side.appendChild(el("h2", { text: "Findings (" + D.findings.length + ")" }));
    side.appendChild(D.findings.length ? el("ul", {}, D.findings.map(findingItem)) : el("p", { class: "loc", text: "None." }));
    if (D.intents.length) {
      side.appendChild(el("h2", { text: "Intents" }));
      side.appendChild(el("ul", {}, D.intents.map(function (i) {
        var imp = i.implementedBy.length ? "implemented" : "not implemented";
        var tst = i.testedBy.length ? "tested" : "untested";
        return el("li", { class: "f", onclick: function () { var t = i.implementedBy[0] || i.testedBy[0]; if (t) focus(t); } }, [
          el("strong", { text: i.id + " " }), el("span", { text: i.text }),
          el("div", { class: "loc", text: imp + " · " + tst })
        ]);
      })));
    }
  }
  function groupDetails(gid) {
    var g = groups[gid]; if (!g) return;
    side.innerHTML = "";
    side.appendChild(backLink());
    side.appendChild(el("h2", { text: g.layer ? "layer" : "directory" }));
    side.appendChild(el("p", {}, [el("strong", { text: g.label }), " ", el("span", { class: "loc", text: g.count + " modules" })]));
    var fs = D.findings.filter(function (f) {
      var n = f.pinned && byId[f.pinned];
      return n && ancestors(treeOf[n.file]).indexOf(g) >= 0;
    });
    if (fs.length) { side.appendChild(el("h2", { text: "Findings" })); side.appendChild(el("ul", {}, fs.map(findingItem))); }
  }
  function details(id) {
    var n = byId[id];
    if (!n) { if (groups[id]) groupDetails(id); else overview(); return; }
    side.innerHTML = "";
    side.appendChild(backLink());
    side.appendChild(el("h2", { text: n.kind + (n.layer ? " · " + n.layer : "") }));
    side.appendChild(el("p", {}, [el("strong", { text: n.name }), " ", el("span", { class: n.status, text: n.status })]));
    side.appendChild(el("div", { class: "loc" }, [editorLink(n.file, n.line)]));
    side.appendChild(el("div", { class: "loc", text: "complexity " + n.complexity + " · " + n.loc + " lines" }));
    var btns = el("div", { class: "btns" });
    btns.appendChild(el("button", { text: "Focus", onclick: function () { focus(id); } }));
    if (mode === "focus" && focusId === id) {
      btns.appendChild(el("button", { text: focusDepth === 1 ? "Depth 2" : "Depth 1", onclick: function () { focusDepth = 3 - focusDepth; render(); details(id); } }));
    }
    if (mode !== "architecture") {
      btns.appendChild(el("button", { text: "Show in architecture", onclick: function () {
        setScope(treeOf[n.file]); cy.getElementById(n.file).select();
      } }));
    }
    side.appendChild(btns);
    if (n.intents.length) side.appendChild(el("p", {}, n.intents.map(function (t) { return el("span", { class: "tag", text: t }); })));
    var fs = n.module ? D.findings.filter(function (f) { return f.file === n.file; }) : (findingsByNode[id] || []);
    if (fs.length) { side.appendChild(el("h2", { text: "Findings" })); side.appendChild(el("ul", {}, fs.map(findingItem))); }
    var ins = D.edges.filter(function (e) { return e.to === id && e.from !== id; });
    var outs = D.edges.filter(function (e) { return e.from === id && e.to !== id; });
    function edgeList(title, list, key) {
      if (!list.length) return;
      side.appendChild(el("h2", { text: title + " (" + list.length + ")" }));
      side.appendChild(el("ul", {}, list.slice(0, 50).map(function (e) {
        var o = byId[e[key]] || { name: e[key], file: "" };
        return el("li", { class: "f", onclick: function () { focus(e[key]); } }, [
          el("span", { text: e.kind + " " }), el("strong", { text: o.name }),
          e.violation ? el("span", { class: "sev error", text: " violation" }) : null,
          el("div", { class: "loc", text: o.file })
        ]);
      })));
    }
    edgeList("Used by", ins, "from");
    edgeList("Uses", outs, "to");
    if (n.before !== undefined || n.after !== undefined) {
      side.appendChild(el("h2", { text: "Before / after" }));
      var panes = el("div", { class: "panes" });
      if (n.before !== undefined) panes.appendChild(el("div", {}, [el("div", { class: "loc removed", text: "before" }), el("pre", { text: n.before })]));
      if (n.after !== undefined) panes.appendChild(el("div", {}, [el("div", { class: "loc added", text: "after" }), el("pre", { text: n.after })]));
      side.appendChild(panes);
    }
  }

  // ---- Search ----
  var search = document.getElementById("search"), list = document.getElementById("ids");
  D.nodes.forEach(function (n) { if (n.kind !== "test") list.appendChild(el("option", { value: n.id })); });
  search.addEventListener("keydown", function (e) {
    if (e.key !== "Enter") return;
    var q = search.value.trim(); if (!q) return;
    var hit = byId[q] && q;
    if (!hit) {
      var lq = q.toLowerCase(), best = null;
      D.nodes.forEach(function (n) {
        var score = n.name.toLowerCase() === lq ? 0 : n.id.toLowerCase().indexOf(lq) >= 0 ? 1 : -1;
        if (score >= 0 && (!best || score < best.s || (score === best.s && n.id.length < best.n.id.length))) best = { n: n, s: score };
      });
      hit = best && best.n.id;
    }
    if (hit) focus(hit);
  });

  var st = document.getElementById("status");
  st.textContent = D.blocking ? "blocking" : "not blocking";
  st.className = "status " + (D.blocking ? "block" : "ok");
  var c = D.counts;
  document.getElementById("counts").textContent = c.added + " added · " + c.changed + " changed · " + c.removed + " removed";
  // Skip a top level with a single box, so the first screen is never one box.
  while (Object.keys(scope.children).length === 1 && scope.mods.length === 0) scope = scope.children[Object.keys(scope.children)[0]];
  render();
  overview();

  if (window.IG_LIVE && window.EventSource) {
    new EventSource("/events").onmessage = function () { location.reload(); };
  }
})();
