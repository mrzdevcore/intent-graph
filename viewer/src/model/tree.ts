// The file tree: an IDE-like explorer of folders, files and their members, as flat rows.

import type { Status, ViewNode } from "../data";
import { base, moduleStatus, type Index } from "./graph";

export interface Folder {
  name: string;
  path: string;
  dirs: Record<string, Folder>;
  files: string[];
  count: number;
  dirty: boolean;
  findings: number;
}

export interface FileTree {
  root: Folder;
  /** Members by parent id, in line order. */
  childrenOf: Record<string, ViewNode[]>;
}

export function fileTree(ix: Index): FileTree {
  const folder = (name: string, path: string): Folder => ({ name, path, dirs: {}, files: [], count: 0, dirty: false, findings: 0 });
  const root = folder("", "");
  Object.keys(ix.modules).sort().forEach((f) => {
    const parts = f.split("/");
    let t = root;
    parts.slice(0, -1).forEach((p, i) => {
      t = t.dirs[p] || (t.dirs[p] = folder(p, parts.slice(0, i + 1).join("/")));
    });
    t.files.push(f);
  });
  // Single-child folder chains read as one row ("src/application").
  (function merge(t: Folder) {
    Object.keys(t.dirs).forEach((k) => {
      let d = t.dirs[k];
      while (Object.keys(d.dirs).length === 1 && d.files.length === 0) {
        const c = d.dirs[Object.keys(d.dirs)[0]];
        d = { ...folder(d.name + "/" + c.name, c.path), dirs: c.dirs, files: c.files };
      }
      t.dirs[k] = d;
      merge(d);
    });
  })(root);
  (function stats(t: Folder) {
    t.count = t.files.length;
    t.dirty = false;
    t.findings = 0;
    t.files.forEach((f) => {
      t.dirty = t.dirty || ix.modules[f].dirty;
      t.findings += ix.modules[f].findings;
    });
    Object.values(t.dirs).forEach((d) => {
      stats(d);
      t.count += d.count;
      t.dirty = t.dirty || d.dirty;
      t.findings += d.findings;
    });
  })(root);
  const childrenOf: Record<string, ViewNode[]> = {};
  ix.D.nodes.forEach((n) => {
    if (!n.module && n.parent && n.kind !== "system") (childrenOf[n.parent] = childrenOf[n.parent] || []).push(n);
  });
  Object.values(childrenOf).forEach((c) => c.sort((a, b) => a.line - b.line));
  return { root, childrenOf };
}

export interface Row {
  depth: number;
  /** A folder's path, or a file's or member's node id. */
  key: string;
  dir: boolean;
  name: string;
  kind?: string;
  status: Status;
  findings: number;
  /** Has children to open. */
  parent: boolean;
  open: boolean;
  title: string;
}

export interface TreeState {
  filter: string;
  expanded: Record<string, boolean>;
  /** Only these files, when "only what is on screen" is on. */
  onScreen: Record<string, boolean> | null;
}

/** The visible rows: at each level, folders, then files, each file followed by its open members. */
export function treeRows(ix: Index, tree: FileTree, st: TreeState): Row[] {
  const q = st.filter.trim().toLowerCase();
  const matches = (text: string) => !q || text.toLowerCase().indexOf(q) >= 0;
  const fileMatches = (f: string) => {
    if (st.onScreen && !st.onScreen[f]) return false;
    if (matches(f)) return true;
    return (ix.modules[f].members || []).some((m) => matches(m.name));
  };
  const dirMatches = (d: Folder): boolean => d.files.some(fileMatches) || Object.values(d.dirs).some(dirMatches);
  const out: Row[] = [];
  const memberRows = (n: ViewNode, depth: number) => {
    const kids = tree.childrenOf[n.id] || [];
    if (!matches(n.name) && !kids.some((c) => matches(c.name)) && q && !matches(n.file)) return;
    const open = !!st.expanded[n.id] || (!!q && kids.length > 0);
    out.push({ depth, key: n.id, dir: false, name: n.name, kind: n.kind, status: n.status, findings: (ix.findingsByNode[n.id] || []).length,
      parent: kids.length > 0, open, title: n.id });
    if (open) kids.forEach((c) => memberRows(c, depth + 1));
  };
  const dirRows = (d: Folder, depth: number) => {
    Object.keys(d.dirs).sort().forEach((k) => {
      const c = d.dirs[k];
      if (!dirMatches(c)) return;
      const open = !!st.expanded[c.path] || !!q || (!!st.onScreen && depth < 6);
      out.push({ depth, key: c.path, dir: true, name: c.name, kind: String(c.count), status: c.dirty ? "changed" : "unchanged",
        findings: c.findings, parent: true, open, title: c.path });
      if (open) dirRows(c, depth + 1);
    });
    d.files.filter(fileMatches).forEach((f) => {
      const kids = tree.childrenOf[f] || [];
      const open = !!st.expanded[f] || (!!q && kids.length > 0);
      out.push({ depth, key: f, dir: false, name: base(f), status: moduleStatus(ix, f), findings: ix.modules[f].findings,
        parent: kids.length > 0, open, title: f });
      if (open) kids.forEach((n) => memberRows(n, depth + 1));
    });
  };
  dirRows(tree.root, 0);
  return out;
}

/** The files behind the graph's elements: a node's file, or every module inside a group. */
export function onScreenFiles(ix: Index, ids: string[]): Record<string, boolean> {
  const on: Record<string, boolean> = {};
  ids.forEach((id) => {
    const b = ix.byId[id];
    if (b && b.file) {
      on[b.file] = true;
      return;
    }
    const g = ix.groups[id];
    if (g)
      (function all(t) {
        t.mods.forEach((m) => (on[m] = true));
        Object.values(t.children).forEach(all);
      })(g);
  });
  return on;
}

/** The rows to open so that node `id` is visible. Merged folder rows use the deepest path of the
 *  chain, so every prefix is opened. */
export function revealKeys(ix: Index, id: string): string[] {
  const n = ix.byId[id];
  if (!n || n.kind === "system") return [];
  const keys: string[] = [];
  const parts = n.file.split("/");
  for (let i = 1; i < parts.length; i++) keys.push(parts.slice(0, i).join("/"));
  if (!n.module) {
    keys.push(n.file);
    let p = n.parent;
    while (p && ix.byId[p] && !ix.byId[p].module) {
      keys.push(p);
      p = ix.byId[p].parent;
    }
  }
  return keys;
}
