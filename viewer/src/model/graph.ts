// Graph indexing: nodes by id, modules with their members, and the drill-down groups
// (layer → directories → module) the Architecture mode walks.

import type { Status, ViewData, ViewEdge, ViewFinding, ViewNode } from "../data";

export interface Module {
  node: ViewNode;
  members: ViewNode[];
  /** Something in the module is not unchanged. */
  dirty: boolean;
  findings: number;
}

export interface Group {
  id: string;
  label: string;
  children: Record<string, Group>;
  mods: string[];
  parent: Group | null;
  layer?: boolean;
  count: number;
  dirty: boolean;
  findings: number;
}

/** A Cytoscape element, as the model builds it. */
export interface ElData {
  id: string;
  label?: string;
  kind?: string;
  status?: Status;
  planned?: boolean;
  findings?: number;
  parent?: string;
  outer?: boolean;
  count?: number;
  bp?: boolean;
  w?: number;
  source?: string;
  target?: string;
  violation?: boolean;
  guessed?: boolean;
  ctx?: boolean;
  labels?: string[];
}
export interface El {
  group: "nodes" | "edges";
  data: ElData;
  position?: { x: number; y: number };
  selectable?: boolean;
  grabbable?: boolean;
}

export interface AggEdge extends ElData {
  source: string;
  target: string;
  count: number;
  violation: boolean;
  status: Status;
  guessed: boolean;
  planned: boolean;
}

export interface Index {
  D: ViewData;
  byId: Record<string, ViewNode>;
  findingsByNode: Record<string, ViewFinding[]>;
  modules: Record<string, Module>;
  root: Group;
  treeOf: Record<string, Group>;
  groups: Record<string, Group>;
  /** Non-import, non-removed edges by source. */
  outEdges: Record<string, ViewEdge[]>;
  /** Member ids by file. */
  membersOf: Record<string, string[]>;
}

export function base(p: string): string {
  return p.split("/").pop() ?? p;
}

function push<T>(o: Record<string, T[]>, k: string, v: T) {
  (o[k] = o[k] || []).push(v);
}

export function index(D: ViewData): Index {
  const byId: Record<string, ViewNode> = {};
  D.nodes.forEach((n) => (byId[n.id] = n));
  const findingsByNode: Record<string, ViewFinding[]> = {};
  D.findings.forEach((f) => {
    if (f.pinned) push(findingsByNode, f.pinned, f);
  });

  const modules: Record<string, Module> = {};
  const moduleFor = (file: string): Module => {
    if (!modules[file]) {
      const n: ViewNode = byId[file] || {
        id: file, name: base(file), file, kind: "module", status: "unchanged", module: true, intents: [], layer: null,
        line: 1, endLine: 1, complexity: 1, loc: 0, parent: null, lane: null, test: false,
      };
      byId[file] = byId[file] || n;
      modules[file] = { node: n, members: [], dirty: false, findings: 0 };
    }
    return modules[file];
  };
  D.nodes.forEach((n) => {
    if (n.kind === "system") return;
    const m = moduleFor(n.file);
    if (!n.module) m.members.push(n);
    if (n.status !== "unchanged") m.dirty = true;
    m.findings += (findingsByNode[n.id] || []).length;
  });

  const root = newGroup("g:", "", null);
  const treeOf: Record<string, Group> = {};
  const child = (t: Group, key: string, label: string) => (t.children[key] = t.children[key] || newGroup(t.id + "/" + key, label, t));
  const byLayer: Record<string, string[]> = {};
  Object.keys(modules).sort().forEach((file) => push(byLayer, modules[file].node.layer || "", file));
  Object.keys(byLayer).forEach((l) => {
    const files = byLayer[l];
    let top = root;
    let prefix: string[] = [];
    if (l) {
      top = child(root, "L:" + l, l);
      top.layer = true;
      // Directories are relative to the deepest directory all of the layer's files share.
      prefix = files[0].split("/").slice(0, -1);
      files.forEach((f) => {
        const d = f.split("/").slice(0, -1);
        let i = 0;
        while (i < prefix.length && i < d.length && prefix[i] === d[i]) i++;
        prefix = prefix.slice(0, i);
      });
    }
    files.forEach((f) => {
      let t = top;
      f.split("/").slice(prefix.length, -1).forEach((d) => (t = child(t, d, d)));
      t.mods.push(f);
      treeOf[f] = t;
    });
  });
  // Merge single-child directory chains ("a" → "b" → "c" becomes "a/b/c").
  (function compress(t: Group) {
    Object.keys(t.children).forEach((k) => {
      let c = t.children[k];
      while (Object.keys(c.children).length === 1 && c.mods.length === 0) {
        const g = c.children[Object.keys(c.children)[0]];
        if (c.layer) {
          c.children = g.children;
          c.mods = g.mods;
          Object.keys(g.children).forEach((x) => (g.children[x].parent = c));
          g.mods.forEach((m) => (treeOf[m] = c));
          continue;
        }
        g.label = c.label + "/" + g.label;
        g.parent = t;
        t.children[k] = g;
        c = g;
      }
      compress(c);
    });
  })(root);
  // A directory holding a single module and nothing else is shown as that module.
  (function hoist(t: Group) {
    Object.keys(t.children).forEach((k) => {
      const c = t.children[k];
      hoist(c);
      if (!c.layer && Object.keys(c.children).length === 0 && c.mods.length === 1) {
        t.mods.push(c.mods[0]);
        treeOf[c.mods[0]] = t;
        delete t.children[k];
      }
    });
  })(root);
  const groups: Record<string, Group> = {};
  (function stats(t: Group) {
    groups[t.id] = t;
    t.count = t.mods.length;
    t.dirty = false;
    t.findings = 0;
    t.mods.forEach((m) => {
      t.dirty = t.dirty || modules[m].dirty;
      t.findings += modules[m].findings;
    });
    Object.values(t.children).forEach((c) => {
      stats(c);
      t.count += c.count;
      t.dirty = t.dirty || c.dirty;
      t.findings += c.findings;
    });
  })(root);

  const outEdges: Record<string, ViewEdge[]> = {};
  D.edges.forEach((e) => {
    if (e.kind !== "imports" && e.status !== "removed") push(outEdges, e.from, e);
  });
  const membersOf: Record<string, string[]> = {};
  D.nodes.forEach((n) => {
    if (!n.module && n.kind !== "system") push(membersOf, n.file, n.id);
  });
  return { D, byId, findingsByNode, modules, root, treeOf, groups, outEdges, membersOf };
}

function newGroup(id: string, label: string, parent: Group | null): Group {
  return { id, label, children: {}, mods: [], parent, count: 0, dirty: false, findings: 0 };
}

/** The visible member for a node: its top-level ancestor inside the module. */
export function topMember(ix: Index, n: ViewNode): ViewNode {
  let cur = n;
  while (cur.parent && ix.byId[cur.parent] && !ix.byId[cur.parent].module) cur = ix.byId[cur.parent];
  return cur;
}

/** The groups from the root (exclusive) down to `t`. */
export function ancestors(ix: Index, t: Group | null | undefined): Group[] {
  const a: Group[] = [];
  while (t && t !== ix.root) {
    a.unshift(t);
    t = t.parent;
  }
  return a;
}

export function nodeData(ix: Index, n: ViewNode, parent?: string): ElData {
  return {
    id: n.id, label: n.name,
    kind: n.module ? "module" : n.kind === "system" ? "system" : n.kind === "route" ? "route" : "member",
    status: n.status, planned: !!n.planned, findings: (ix.findingsByNode[n.id] || []).length, parent,
  };
}

/** Edges between the boxes `repOf` maps their ends to, counted and merged. */
export function aggregateEdges(ix: Index, repOf: (id: string) => string | null, keep: (id: string) => boolean): AggEdge[] {
  const agg: Record<string, AggEdge> = {};
  const out: AggEdge[] = [];
  ix.D.edges.forEach((e) => {
    const a = repOf(e.from);
    const b = repOf(e.to);
    if (!a || !b || a === b || !keep(a) || !keep(b)) return;
    const k = a + "|" + b;
    let x = agg[k];
    if (!x) {
      x = agg[k] = { id: "", source: a, target: b, count: 0, violation: false, status: "unchanged", guessed: true, planned: true };
      out.push(x);
    }
    x.count++;
    x.violation = x.violation || e.violation;
    x.guessed = x.guessed && e.confidence === "guessed";
    x.planned = x.planned && !!e.planned;
    if (e.status !== "unchanged" && x.status === "unchanged") x.status = e.status;
  });
  return out;
}

/** Search: an exact id, else an exact name, else the shortest id containing the query. */
export function findNode(ix: Index, q: string): string | null {
  q = q.trim();
  if (!q) return null;
  if (ix.byId[q]) return q;
  const lq = q.toLowerCase();
  let best: { n: ViewNode; s: number } | null = null;
  ix.D.nodes.forEach((n) => {
    const score = n.name.toLowerCase() === lq ? 0 : n.id.toLowerCase().indexOf(lq) >= 0 ? 1 : -1;
    if (score >= 0 && (!best || score < best.s || (score === best.s && n.id.length < best.n.id.length))) best = { n, s: score };
  });
  return best ? (best as { n: ViewNode }).n.id : null;
}

/** The status a module is drawn with: its own, or "changed" when something inside it is. */
export function moduleStatus(ix: Index, f: string): Status {
  const m = ix.modules[f];
  return m.node.status !== "unchanged" ? m.node.status : m.dirty ? "changed" : "unchanged";
}
