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
    if (n.kind === "system") return;
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
    return { id: n.id, label: n.name, kind: n.module ? "module" : n.kind === "system" ? "system" : n.kind === "route" ? "route" : "member", status: n.status,
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
    if (n.kind === "system") return id;
    var path = ancestors(treeOf[n.file]), chain = ancestors(scope);
    for (var i = 0; i < chain.length; i++) {
      if (i >= path.length) return n.file;
      if (path[i] !== chain[i]) return path[i].id;
    }
    return path.length > chain.length ? path[chain.length].id : n.file;
  }
  function insideScope(repId) {
    if (byId[repId] && byId[repId].kind === "system") return false;
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
      if (byId[id] && byId[id].kind === "system") {
        var sd = nodeData(byId[id]); sd.kind = "system"; sd.outer = outer;
        els.push({ group: "nodes", data: sd });
      } else if (g) {
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
      if (n.module || n.kind === "system") return;
      if (!shown[n.file]) { var m = byId[n.file]; els.push({ group: "nodes", data: { id: n.file, label: base(n.file), kind: "openmodule", status: m.status } }); shown[n.file] = true; }
      els.push({ group: "nodes", data: nodeData(n, n.file) }); shown[id] = true;
    });
    Object.keys(ids).forEach(function (id) {
      var n = byId[id];
      if (n && (n.module || n.kind === "system") && !shown[id]) {
        var d = nodeData(n); if (n.kind === "system") d.kind = "system";
        els.push({ group: "nodes", data: d }); shown[id] = true;
      }
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

  // ---- Blueprint: one page, then everything behind it, in swimlanes ----
  var journey = null, uiOnly = false;
  var lastLane = D.lanes.length - 1;
  function laneOf(n) { return n.lane === null || n.lane === undefined ? null : n.lane; }
  // The box a node is drawn as: endpoints and systems themselves, everything else its module.
  function unitOf(id) {
    var n = byId[id]; if (!n) return null;
    return n.kind === "system" || n.kind === "route" ? id : n.file;
  }
  var outEdges = {};
  D.edges.forEach(function (e) { if (e.kind !== "imports" && e.status !== "removed") (outEdges[e.from] = outEdges[e.from] || []).push(e); });
  var membersOf = {};
  D.nodes.forEach(function (n) { if (!n.module && n.kind !== "system") (membersOf[n.file] = membersOf[n.file] || []).push(n.id); });

  // "front/app/src/pages/accounts/[id]/index.vue" → app "front/app", route "/accounts/[id]".
  function journeyName(file) {
    var i = file.indexOf("/pages/");
    if (i < 0) {
      // Not a file-routed page (a view): app = the path before src/, name = the file without extension.
      var j = file.indexOf("/src/");
      return { app: j < 0 ? file.split("/").slice(0, -1).join("/") : file.slice(0, j), route: base(file).replace(/\.[a-z]+$/, "") };
    }
    var route = file.slice(i + 6).replace(/\.[a-z]+$/, "").replace(/\/index$/, "") || "/";
    var app = file.slice(0, i).replace(/\/(src|app)(\/application)?$/, "");
    return { app: app, route: route };
  }

  function journeys() {
    return D.nodes.filter(function (n) { return n.module && n.lane === 0 && n.status !== "removed"; })
      .sort(function (a, b) { return a.file < b.file ? -1 : 1; });
  }

  // Walks forward from the page. Lanes only go down (except within a lane), other pages are not
  // entered, and a node counts as backstage once an http edge or a system has been reached.
  function blueprintGraph(page) {
    var seen = {}, backstage = {}, unitEdges = {}, order = [];
    var queue = [page].concat(membersOf[page] || []).map(function (id) { return { id: id, back: false }; });
    function visit(id, back) {
      var key = id + (back ? "|b" : "");
      if (seen[key]) return false;
      seen[key] = true;
      var u = unitOf(id);
      if (u && order.indexOf(u) < 0) order.push(u);
      if (back) backstage[u] = true;
      return true;
    }
    queue.forEach(function (q) { visit(q.id, false); });
    while (queue.length) {
      var q = queue.shift();
      var from = byId[q.id]; if (!from) continue;
      var fromUnit = unitOf(q.id);
      var next = (outEdges[q.id] || []).filter(function (e) { return byId[e.to] && !byId[e.to].test; });
      // A function's module-level code (imports of systems, markup) belongs to the journey too.
      if (!from.module && from.kind !== "system" && from.kind !== "route") next.push({ to: from.file, kind: "member", from: q.id });
      next.forEach(function (e) {
        var to = byId[e.to]; if (!to) return;
        var lf = laneOf(byId[fromUnit] || from), lt = laneOf(to);
        if (lt === 0 && to.file !== page) return;
        if (lf !== null && lt !== null && lt < lf) return;
        var back = q.back || e.kind === "http" || to.kind === "system";
        var tu = unitOf(e.to);
        if (fromUnit !== tu && e.kind !== "member") {
          var k = fromUnit + "|" + tu;
          var x = unitEdges[k] || (unitEdges[k] = { source: fromUnit, target: tu, count: 0, kind: e.kind, labels: [], violation: false });
          x.count++; x.violation = x.violation || e.violation;
          if (e.kind === "http") { x.kind = "http"; }
        }
        if (visit(e.to, back)) queue.push({ id: e.to, back: back });
      });
    }
    // Keep only units that lead to the backstage (unless showing UI-only components).
    var keep = {};
    order.forEach(function (u) { keep[u] = uiOnly || backstage[u] || u === page; });
    if (!uiOnly) {
      var changed = true;
      while (changed) {
        changed = false;
        Object.keys(unitEdges).forEach(function (k) {
          var e = unitEdges[k];
          if (keep[e.target] && !keep[e.source]) { keep[e.source] = true; changed = true; }
        });
      }
    }
    return { units: order.filter(function (u) { return keep[u]; }), edges: Object.keys(unitEdges).map(function (k) { return unitEdges[k]; }).filter(function (e) { return keep[e.source] && keep[e.target]; }) };
  }

  function blueprint() {
    var els = [];
    if (!journey) return els;
    var bg = blueprintGraph(journey);
    var rows = {}, lanes = D.lanes.concat(["Other"]), other = lanes.length - 1;
    bg.units.forEach(function (u) {
      var l = laneOf(byId[u]); if (l === null) l = other;
      (rows[l] = rows[l] || []).push(u);
    });
    // Order each lane by the average x of what points at it, top-down, to limit crossings.
    var x = {}, preds = {};
    bg.edges.forEach(function (e) { (preds[e.target] = preds[e.target] || []).push(e.source); });
    var used = Object.keys(rows).map(Number).sort(function (a, b) { return (a === other ? lastLane + 0.5 : a) - (b === other ? lastLane + 0.5 : b); });
    var gapX = 210, gapY = 150, width = 0;
    function labelOf(u) { var n = byId[u]; return n.kind === "route" || n.kind === "system" ? n.name : u === journey ? journeyName(u).route : base(n.file); }
    function boxW(u) { return Math.max(150, Math.min(labelOf(u).length, 26) * 8.5 + 40); }
    used.forEach(function (l, row) {
      var us = rows[l];
      us.forEach(function (u, i) {
        var ps = (preds[u] || []).filter(function (p) { return x[p] !== undefined; });
        us[i] = { u: u, k: ps.length ? ps.reduce(function (s, p) { return s + x[p]; }, 0) / ps.length : i * gapX };
      });
      us.sort(function (a, b) { return a.k - b.k; });
      var cx = 0;
      us.forEach(function (o) { var w = boxW(o.u); x[o.u] = cx + w / 2; cx += w; });
      rows[l] = us.map(function (o) { return o.u; });
      rows[l].w = cx;
      width = Math.max(width, cx);
    });
    used.forEach(function (l, row) {
      var y = row * gapY;
      var offset = (width - rows[l].w) / 2;
      els.push({ group: "nodes", data: { id: "lane:" + l, label: lanes[l], kind: "lane", w: width + 300 }, position: { x: width / 2 - 60, y: y }, selectable: false, grabbable: false });
      els.push({ group: "nodes", data: { id: "lanelabel:" + l, label: lanes[l], kind: "lanelabel" }, position: { x: -150, y: y }, selectable: false, grabbable: false });
      rows[l].forEach(function (u) {
        var n = byId[u], d = nodeData(n);
        d.label = labelOf(u);
        d.findings = n.module && modules[u] ? modules[u].findings : d.findings;
        if (n.module && modules[u] && modules[u].dirty && d.status === "unchanged") d.status = "changed";
        d.kind = n.kind === "system" ? "system" : n.kind === "route" ? "route" : u === journey ? "journey" : "module";
        d.bp = true;
        if (u === journey) d.label = journeyName(u).route;
        els.push({ group: "nodes", data: d, position: { x: x[u] + offset, y: y } });
      });
    });
    bg.edges.forEach(function (e, i) {
      e.id = "b" + i; e.status = "unchanged"; e.guessed = false;
      els.push({ group: "edges", data: e });
    });
    return els;
  }

  function journeyList() {
    side.innerHTML = "";
    side.appendChild(el("h2", { text: "Journeys" }));
    if (!D.lanes.length) {
      side.appendChild(el("p", { class: "loc", text: "Add a blueprint section to ig.yaml (lanes, http, systems) to see journeys." }));
      return;
    }
    var js = journeys();
    if (!js.length) { side.appendChild(el("p", { class: "loc", text: "No module is in the first lane (" + D.lanes[0] + ")." })); return; }
    var opt = el("label", { class: "loc" }, [el("input", { type: "checkbox", onchange: function (ev) { uiOnly = ev.target.checked; if (journey) render(); } }), " show UI-only components"]);
    if (uiOnly) opt.firstChild.checked = true;
    side.appendChild(opt);
    var app = null;
    js.forEach(function (n) {
      var jn = journeyName(n.file);
      if (jn.app !== app) { app = jn.app; side.appendChild(el("h2", { text: app || "pages" })); }
      side.appendChild(el("ul", {}, [el("li", { class: "f" + (n.id === journey ? " current" : ""), onclick: function () { journey = n.id; render(); journeyList(); } }, [
        el("strong", { text: jn.route })
      ])]));
    });
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
      { selector: "node[?bp]", style: { "font-size": 14, "min-zoomed-font-size": 4, "text-max-width": 230, "text-wrap": "ellipsis" } },
      { selector: "edge[kind = 'dispatch']", style: { "line-style": "dashed" } },
      { selector: "node[kind = 'lane']", style: {
        "shape": "rectangle", "width": "data(w)", "height": 120, "background-color": line, "background-opacity": 0.18,
        "border-width": 0, "label": "", "events": "no", "z-index": 0 } },
      { selector: "node[kind = 'lanelabel']", style: {
        "shape": "rectangle", "width": 1, "height": 1, "background-opacity": 0, "border-width": 0, "text-valign": "center",
        "text-halign": "center", "font-size": 18, "font-weight": 700, "color": muted, "events": "no", "min-zoomed-font-size": 3 } },
      { selector: "node[kind = 'journey']", style: { "shape": "round-rectangle", "width": 34, "height": 34, "background-color": accent, "font-size": 12, "font-weight": 700 } },
      { selector: "node[kind = 'route']", style: { "shape": "tag", "width": 22, "height": 22, "background-color": css("--note") } },
      { selector: "node[kind = 'system']", style: { "shape": "barrel", "width": 38, "height": 44, "background-color": css("--warning"), "font-size": 12, "font-weight": 700 } },
      { selector: "edge[kind = 'http']", style: { "line-color": css("--note"), "target-arrow-color": css("--note"), "line-style": "dashed", "opacity": 0.95, "width": 2 } },
      { selector: "edge[kind = 'uses']", style: { "line-color": css("--warning"), "target-arrow-color": css("--warning"), "opacity": 0.9 } },
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
  var lastMode = null, lastTreeMode = null;
  function render(zoomTo) {
    var els = mode === "architecture" ? architecture() : mode === "focus" ? exact(focusIds()) : mode === "blueprint" ? blueprint() : exact(changedIds());
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
    var layout = cy.layout(mode === "blueprint" ? { name: "preset", fit: false } : window.cytoscapeFcose ? {
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
    if (mode === "blueprint") journeyList();
    if (window.igTreeReady) {
      if (mode !== lastTreeMode) { treeInView.checked = mode !== "architecture"; lastTreeMode = mode; }
      computeOnScreen(); renderTree();
    }
    hint.textContent = mode === "blueprint" ? (journey ? "Journey " + journeyName(journey).route + ": the page and everything behind it, by lane. Dashed blue: HTTP calls." : "Pick a page on the right.") :
      mode === "architecture" ? "Click a box to open it. Dimmed boxes are outside, connected context." :
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
  function details(id, fromTree) {
    var n = byId[id];
    if (!n) { if (groups[id]) groupDetails(id); else overview(); return; }
    if (!fromTree && window.igTreeReady) revealInTree(id);
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


  // ---- Tree: an IDE-like explorer of folders, files and functions, synced with the graph ----
  var treeBody = document.getElementById("tree-body"), treeFilter = document.getElementById("tree-filter"),
      treeInView = document.getElementById("tree-inview");
  var fileTree = { name: "", path: "", dirs: {}, files: [] };
  Object.keys(modules).sort().forEach(function (f) {
    var parts = f.split("/"), t = fileTree;
    parts.slice(0, -1).forEach(function (p, i) {
      t = t.dirs[p] || (t.dirs[p] = { name: p, path: parts.slice(0, i + 1).join("/"), dirs: {}, files: [] });
    });
    t.files.push(f);
  });
  // Single-child folder chains read as one row ("src/application").
  (function merge(t) {
    Object.keys(t.dirs).forEach(function (k) {
      var d = t.dirs[k];
      while (Object.keys(d.dirs).length === 1 && d.files.length === 0) {
        var c = d.dirs[Object.keys(d.dirs)[0]];
        d = { name: d.name + "/" + c.name, path: c.path, dirs: c.dirs, files: c.files };
      }
      t.dirs[k] = d;
      merge(d);
    });
  })(fileTree);
  (function stats(t) {
    t.count = t.files.length; t.dirty = false; t.findings = 0;
    t.files.forEach(function (f) { t.dirty = t.dirty || modules[f].dirty; t.findings += modules[f].findings; });
    Object.keys(t.dirs).forEach(function (k) { var d = t.dirs[k]; stats(d); t.count += d.count; t.dirty = t.dirty || d.dirty; t.findings += d.findings; });
  })(fileTree);
  var childrenOf = {};
  D.nodes.forEach(function (n) { if (!n.module && n.parent && n.kind !== "system") (childrenOf[n.parent] = childrenOf[n.parent] || []).push(n); });
  Object.keys(childrenOf).forEach(function (k) { childrenOf[k].sort(function (a, b) { return a.line - b.line; }); });

  var expanded = {}, treeSel = null, onScreen = null;
  function statusOf(f) { var m = modules[f]; return m.node.status !== "unchanged" ? m.node.status : m.dirty ? "changed" : "unchanged"; }
  function matches(text) { var q = treeFilter.value.trim().toLowerCase(); return !q || text.toLowerCase().indexOf(q) >= 0; }
  function fileMatches(f) {
    if (onScreen && !onScreen[f]) return false;
    if (matches(f)) return true;
    return (modules[f].members || []).some(function (m) { return matches(m.name); });
  }
  function dirMatches(d) {
    return d.files.some(fileMatches) || Object.keys(d.dirs).some(function (k) { return dirMatches(d.dirs[k]); });
  }
  function row(depth, opts) {
    var r = el("div", { class: "row" + (opts.dir ? " dir" : "") + (opts.id && opts.id === treeSel ? " sel" : ""), role: "treeitem",
      style: "padding-left:" + (8 + depth * 12) + "px", title: opts.title || "" }, [
      el("span", { class: "caret", text: opts.caret || "" }),
      opts.status && opts.status !== "unchanged" ? el("span", { class: "dot " + opts.status }) : null,
      el("span", { class: "name", text: opts.name }),
      opts.kind ? el("span", { class: "kind", text: opts.kind }) : null,
      opts.findings ? el("span", { class: "nf", text: "● " + opts.findings }) : null
    ]);
    if (opts.id && opts.id === treeSel) r.setAttribute("aria-selected", "true");
    r.addEventListener("click", opts.onclick);
    if (opts.ondblclick) r.addEventListener("dblclick", opts.ondblclick);
    return r;
  }
  function memberRows(out, n, depth) {
    if (!matches(n.name) && !(childrenOf[n.id] || []).some(function (c) { return matches(c.name); }) && treeFilter.value.trim() && !matches(n.file)) return;
    var kids = childrenOf[n.id] || [], open = expanded[n.id] || (!!treeFilter.value.trim() && kids.length > 0);
    out.push(row(depth, { id: n.id, name: n.name, kind: n.kind, status: n.status, findings: (findingsByNode[n.id] || []).length,
      caret: kids.length ? (open ? "▾" : "▸") : "", title: n.id,
      onclick: function () { if (kids.length) expanded[n.id] = !open; treeSelect(n.id); },
      ondblclick: function () { focus(n.id); } }));
    if (open) kids.forEach(function (c) { memberRows(out, c, depth + 1); });
  }
  function dirRows(out, d, depth) {
    Object.keys(d.dirs).sort().forEach(function (k) {
      var c = d.dirs[k];
      if (!dirMatches(c)) return;
      var open = expanded[c.path] || !!treeFilter.value.trim() || (!!onScreen && depth < 6);
      out.push(row(depth, { dir: true, name: c.name, caret: open ? "▾" : "▸", status: c.dirty ? "changed" : "unchanged", findings: c.findings,
        kind: String(c.count), title: c.path, onclick: function () { expanded[c.path] = !open; renderTree(); highlightFolder(c.path); } }));
      if (open) dirRows(out, c, depth + 1);
    });
    d.files.filter(fileMatches).forEach(function (f) {
      var kids = childrenOf[f] || [], open = expanded[f] || (!!treeFilter.value.trim() && kids.length > 0);
      out.push(row(depth, { id: f, name: base(f), status: statusOf(f), findings: modules[f].findings, caret: kids.length ? (open ? "▾" : "▸") : "",
        title: f, onclick: function () { if (kids.length) expanded[f] = !open; treeSelect(f); }, ondblclick: function () { focus(f); } }));
      if (open) kids.forEach(function (n) { memberRows(out, n, depth + 1); });
    });
  }
  function renderTree() {
    var out = [];
    dirRows(out, fileTree, 0);
    treeBody.innerHTML = "";
    if (!out.length) treeBody.appendChild(el("p", { class: "loc", text: "  Nothing matches." }));
    out.forEach(function (r) { treeBody.appendChild(r); });
    var sel = treeBody.querySelector(".sel");
    if (sel) sel.scrollIntoView({ block: "nearest" });
  }
  // The graph element that stands for a node in the current mode.
  function onGraph(id) {
    var n = byId[id]; if (!n) return null;
    var candidates = mode === "architecture" ? [rep(id)] : mode === "blueprint" ? [unitOf(id)] : [id, topMember(n).id, n.file];
    for (var i = 0; i < candidates.length; i++) { var x = candidates[i] && cy.getElementById(candidates[i]); if (x && x.length) return x; }
    return null;
  }
  function treeSelect(id) {
    treeSel = id;
    renderTree();
    details(id, true);
    var x = onGraph(id);
    cy.$(":selected").unselect();
    cy.elements().removeClass("faded");
    if (x) {
      x.select();
      var nb = x.closedNeighborhood().union(x.ancestors());
      cy.elements().addClass("faded"); nb.removeClass("faded"); nb.ancestors().removeClass("faded");
      cy.animate({ center: { eles: x }, duration: 200 });
    } else {
      hint.textContent = "Not in this view; double-click it in the tree (or press Focus) to see its neighbourhood.";
    }
  }
  function highlightFolder(path) {
    var prefix = path + "/";
    var xs = cy.nodes().filter(function (n) { var b = byId[n.id()]; return !!b && !!b.file && b.file.indexOf(prefix) === 0; });
    cy.elements().removeClass("faded");
    if (xs.length) { cy.elements().addClass("faded"); xs.removeClass("faded"); xs.ancestors().removeClass("faded"); }
  }
  // Graph → tree: reveal and select the file or function.
  function revealInTree(id) {
    var n = byId[id]; if (!n || n.kind === "system") return;
    var parts = n.file.split("/");
    for (var i = 1; i < parts.length; i++) expanded[parts.slice(0, i).join("/")] = true;
    // Merged rows use the deepest path of the chain; mark every prefix so they open too.
    if (!n.module) { expanded[n.file] = true; var p = n.parent; while (p && byId[p] && !byId[p].module) { expanded[p] = true; p = byId[p].parent; } }
    treeSel = id;
    renderTree();
  }
  function computeOnScreen() {
    if (!treeInView.checked) { onScreen = null; return; }
    onScreen = {};
    cy.nodes().forEach(function (x) {
      var b = byId[x.id()];
      if (b && b.file) { onScreen[b.file] = true; return; }
      var g = groups[x.id()];
      if (g) (function all(t) { t.mods.forEach(function (m) { onScreen[m] = true; }); Object.keys(t.children).forEach(function (k) { all(t.children[k]); }); })(g);
    });
  }
  treeFilter.addEventListener("input", renderTree);
  treeInView.addEventListener("change", function () { computeOnScreen(); renderTree(); });

  var st = document.getElementById("status");
  st.textContent = D.blocking ? "blocking" : "not blocking";
  st.className = "status " + (D.blocking ? "block" : "ok");
  var c = D.counts;
  document.getElementById("counts").textContent = c.added + " added · " + c.changed + " changed · " + c.removed + " removed";
  // Skip a top level with a single box, so the first screen is never one box.
  while (Object.keys(scope.children).length === 1 && scope.mods.length === 0) scope = scope.children[Object.keys(scope.children)[0]];
  window.igTreeReady = true;
  render();
  overview();

  if (window.IG_LIVE && window.EventSource) {
    new EventSource("/events").onmessage = function () { location.reload(); };
  }
})();
