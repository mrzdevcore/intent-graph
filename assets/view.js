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

  function moduleOf(id) { var n = byId[id]; return n ? n.file : id; }

  function elements(mode) {
    var nodes = {}, edges = [];
    if (mode === "modules") {
      var status = {};
      D.nodes.forEach(function (n) {
        var s = status[n.file] || "unchanged";
        if (n.module) s = n.status === "unchanged" ? s : n.status;
        else if (n.status !== "unchanged" && s === "unchanged") s = "changed";
        status[n.file] = s;
      });
      Object.keys(status).forEach(function (f) { nodes[f] = { id: f, label: f.split("/").pop(), status: status[f], title: f }; });
      var seen = {};
      D.edges.forEach(function (e) {
        var a = moduleOf(e.from), b = moduleOf(e.to);
        if (a === b) return;
        var k = a + "|" + b;
        if (seen[k]) { seen[k].violation = seen[k].violation || e.violation; return; }
        seen[k] = { source: a, target: b, kind: "module", status: e.status, violation: e.violation };
        edges.push(seen[k]);
      });
    } else {
      var include = {};
      if (mode === "all") {
        D.nodes.forEach(function (n) { include[n.id] = true; });
      } else {
        D.nodes.forEach(function (n) { if (n.status !== "unchanged") include[n.id] = true; });
        var changed = Object.assign({}, include);
        D.edges.forEach(function (e) {
          if (changed[e.from]) include[e.to] = true;
          if (changed[e.to]) include[e.from] = true;
        });
      }
      Object.keys(include).forEach(function (id) {
        var n = byId[id];
        if (n) nodes[id] = { id: id, label: n.module ? n.name : n.name + "\n" + n.file.split("/").pop(), status: n.status, title: id };
      });
      D.edges.forEach(function (e) {
        if (nodes[e.from] && nodes[e.to] && e.from !== e.to) {
          edges.push({ source: e.from, target: e.to, kind: e.kind, status: e.status, violation: e.violation, guessed: e.confidence === "guessed" });
        }
      });
    }
    var els = [];
    Object.keys(nodes).forEach(function (k) {
      var n = nodes[k];
      n.findings = (findingsByNode[k] || []).length;
      els.push({ group: "nodes", data: n });
    });
    edges.forEach(function (e, i) { e.id = "e" + i; els.push({ group: "edges", data: e }); });
    return els;
  }

  var color = { added: css("--added"), changed: css("--changed"), removed: css("--removed"), unchanged: css("--unchanged") };
  var cy = cytoscape({
    container: document.getElementById("cy"),
    wheelSensitivity: 0.3,
    style: [
      { selector: "node", style: {
        "label": "data(label)", "text-wrap": "wrap", "font-size": 10, "color": css("--fg"),
        "background-color": function (n) { return color[n.data("status")] || color.unchanged; },
        "width": 18, "height": 18, "text-valign": "bottom", "text-margin-y": 4 } },
      { selector: "node[status = 'removed']", style: { "border-width": 2, "border-style": "dashed", "border-color": color.removed, "background-opacity": 0.3 } },
      { selector: "node[findings > 0]", style: { "border-width": 3, "border-color": css("--error") } },
      { selector: "node:selected", style: { "border-width": 4, "border-color": css("--accent") } },
      { selector: "edge", style: {
        "width": 1.2, "curve-style": "bezier", "target-arrow-shape": "triangle", "arrow-scale": 0.8,
        "line-color": css("--line"), "target-arrow-color": css("--line") } },
      { selector: "edge[status = 'added']", style: { "line-color": color.added, "target-arrow-color": color.added } },
      { selector: "edge[status = 'removed']", style: { "line-style": "dashed", "line-color": color.removed, "target-arrow-color": color.removed } },
      { selector: "edge[?guessed]", style: { "line-style": "dotted" } },
      { selector: "edge[?violation]", style: { "width": 3, "line-color": css("--error"), "target-arrow-color": css("--error") } }
    ]
  });

  var mode = D.mode;
  function render() {
    cy.elements().remove();
    cy.add(elements(mode));
    var n = cy.nodes().length;
    cy.layout(n > 600 ? { name: "grid", animate: false } : { name: "cose", animate: false, nodeRepulsion: 8000, idealEdgeLength: 80 }).run();
    document.querySelectorAll(".modes button").forEach(function (b) { b.setAttribute("aria-pressed", String(b.dataset.mode === mode)); });
  }
  document.querySelectorAll(".modes button").forEach(function (b) {
    b.addEventListener("click", function () { mode = b.dataset.mode; render(); });
  });

  var side = document.getElementById("side");
  function findingItem(f) {
    return el("li", { class: "f", onclick: function () { if (f.pinned) select(f.pinned); } }, [
      el("span", { class: "sev " + f.severity, text: f.severity }),
      el("span", { text: f.message }),
      el("div", { class: "loc" }, [f.ruleId + " · ", editorLink(f.file, f.line)])
    ]);
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
        return el("li", { class: "f", onclick: function () { var t = i.implementedBy[0] || i.testedBy[0]; if (t) select(t); } }, [
          el("strong", { text: i.id + " " }), el("span", { text: i.text }),
          el("div", { class: "loc", text: imp + " · " + tst })
        ]);
      })));
    }
  }

  function details(id) {
    var n = byId[id];
    if (!n) { overview(); return; }
    side.innerHTML = "";
    side.appendChild(el("p", {}, [el("a", { href: "#", onclick: function (e) { e.preventDefault(); cy.$(":selected").unselect(); overview(); }, text: "← overview" })]));
    side.appendChild(el("h2", { text: n.kind }));
    side.appendChild(el("p", {}, [el("strong", { text: n.name }), " ", el("span", { class: n.status, text: n.status })]));
    side.appendChild(el("div", { class: "loc" }, [editorLink(n.file, n.line)]));
    side.appendChild(el("div", { class: "loc", text: "complexity " + n.complexity + " · " + n.loc + " lines" }));
    if (n.intents.length) side.appendChild(el("p", {}, n.intents.map(function (t) { return el("span", { class: "tag", text: t }); })));
    var fs = findingsByNode[id] || [];
    if (fs.length) {
      side.appendChild(el("h2", { text: "Findings" }));
      side.appendChild(el("ul", {}, fs.map(findingItem)));
    }
    if (n.before !== undefined || n.after !== undefined) {
      side.appendChild(el("h2", { text: "Before / after" }));
      var panes = el("div", { class: "panes" });
      if (n.before !== undefined) panes.appendChild(el("div", {}, [el("div", { class: "loc removed", text: "before" }), el("pre", { text: n.before })]));
      if (n.after !== undefined) panes.appendChild(el("div", {}, [el("div", { class: "loc added", text: "after" }), el("pre", { text: n.after })]));
      side.appendChild(panes);
    }
  }

  function select(id) {
    var target = mode === "modules" ? moduleOf(id) : id;
    if (!cy.getElementById(target).length && mode === "changed") { mode = "all"; render(); }
    var e = cy.getElementById(target);
    cy.$(":selected").unselect();
    if (e.length) { e.select(); cy.animate({ center: { eles: e }, duration: 200 }); }
    details(id);
  }
  cy.on("tap", "node", function (e) { details(e.target.id()); });

  var st = document.getElementById("status");
  st.textContent = D.blocking ? "blocking" : "not blocking";
  st.className = "status " + (D.blocking ? "block" : "ok");
  var c = D.counts;
  document.getElementById("counts").textContent = c.added + " added · " + c.changed + " changed · " + c.removed + " removed";
  render();
  overview();

  if (window.IG_LIVE && window.EventSource) {
    new EventSource("/events").onmessage = function () { location.reload(); };
  }
})();
