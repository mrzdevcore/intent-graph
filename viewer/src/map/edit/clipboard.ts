import type { ArrowKind, Edit, KindView, MapArrow, MapBox, View } from "../types";
import { GRID } from "../nav";
import { inside as innerKey, type Place } from "../place/places";
import { written } from "./menu";

export type CopyBox = {
  id: string; parent: string | null; kind: string; title: string; props: Record<string, string>; context?: string; place?: Place; room?: Place;
  from?: string;
};
export type CopyArrow = { from: string; to: string; kind: ArrowKind; label?: string; note?: string; context?: string };
export type Copy = { boxes: CopyBox[]; arrows: CopyArrow[]; parent?: string };
export type Clip = { copy: Copy; text: string };

// kept outside the map so a map mounted again still pastes what was copied
let last: Clip | null = null;
export const remembered = () => last;
export const remember = (c: Clip | null) => (last = c);

const ARROWS: Record<string, ArrowKind> = {
  calls: "calls", sends: "sends", reads: "reads", writes: "writes", triggers: "triggers", implements: "implements", uses: "uses",
  network: "calls", event: "sends", data: "reads", depends: "uses",
};
const OLD: Record<string, string> = { folder: "group", component: "module", api: "service", worker: "service", library: "module" };
const MAX_ID = 64, MAX_TITLE = 60;

const flat = (a: MapArrow): MapArrow[] => (a.parts?.length ? a.parts.flatMap(flat) : [a]);

export function copy(ids: string[], view: View, inside: boolean, places: Record<string, Place>): Copy {
  const byId = new Map(view.boxes.map((b) => [b.id, b])), kids = new Map<string, MapBox[]>();
  for (const b of view.boxes) if (b.parent && b.parent !== b.id && written(b)) kids.set(b.parent, [...(kids.get(b.parent) ?? []), b]);
  const picked = new Set(ids.filter((id) => written(byId.get(id))));
  const under = (id: string) => {
    for (let p = byId.get(id)?.parent, n = 0; p && n < 64; p = byId.get(p)?.parent, n++) if (picked.has(p)) return true;
    return false;
  };
  const tops = [...picked].filter((id) => !under(id));
  const boxes: CopyBox[] = [], seen = new Set<string>();
  const take = (b: MapBox, parent: string | null, d: number) => {
    if (seen.has(b.id) || d > 64) return;
    seen.add(b.id);
    const props: Record<string, string> = {};
    for (const [k, f] of [["tech", b.tech], ["description", b.description], ["logo", b.logo]] as const) if (f?.value) props[k] = f.value;
    Object.assign(props, b.props ?? {});
    boxes.push({
      id: b.id, parent, kind: b.custom ?? b.kind.value, title: b.title.value, props, ...(parent === null && b.parent ? { from: b.parent } : {}),
      ...(b.context?.value ? { context: b.context.value } : {}),
      ...(places[b.id] ? { place: { ...places[b.id] } } : {}), ...(places[innerKey(b.id)] ? { room: { ...places[innerKey(b.id)] } } : {}),
    });
    const inner = inside ? kids.get(b.id) ?? [] : (kids.get(b.id) ?? []).filter((k) => picked.has(k.id));
    for (const k of inner) take(k, b.id, d + 1);
  };
  for (const id of tops) take(byId.get(id)!, null, 0);
  const arrows: CopyArrow[] = [], keys = new Set<string>();
  for (const a of inside ? view.arrows.filter((x) => !x.ghost).flatMap(flat) : []) {
    const key = `${a.from}>${a.to}`;
    if (a.kind === "bundle" || a.origin === "drift" || a.origin === "unmanaged" || keys.has(key) || !seen.has(a.from) || !seen.has(a.to)) continue;
    keys.add(key);
    arrows.push({ from: a.from, to: a.to, kind: a.kind, ...(a.label ? { label: a.label } : {}), ...(a.note ? { note: a.note } : {}), ...(a.context ? { context: a.context } : {}) });
  }
  const parent = tops.length ? byId.get(tops[0])?.parent : undefined;
  return { boxes, arrows, ...(parent ? { parent } : {}) };
}

const quote = (s: string) =>
  `"${s.replace(/[\\"]/g, (c) => `\\${c}`).replace(/\n/g, "\\n").replace(/\r/g, "\\r").replace(/\t/g, "\\t")
    .replace(/[\u0000-\u001f\u007f]/g, (c) => `\\u{${c.charCodeAt(0).toString(16)}}`)}"`;
const ident = (s: string) => (/^[a-z][a-z0-9-]*$/i.test(s) ? s : quote(s));

export function toKdl(c: Copy): string {
  const kids = new Map<string | null, CopyBox[]>();
  for (const b of c.boxes) kids.set(b.parent, [...(kids.get(b.parent) ?? []), b]);
  const out = (b: CopyBox, pad: string): string => {
    const props = [`title=${quote(b.title)}`, ...Object.entries(b.props).map(([k, v]) => `${ident(k)}=${quote(v)}`)];
    const body = [
      ...(b.context ? [`context ${quote(b.context)}`] : []),
      ...c.arrows.filter((a) => a.from === b.id).map((a) => [`${a.kind} ${quote(a.to)}`, ...(["label", "note", "context"] as const).flatMap((k) => (a[k] ? [`${k}=${quote(a[k]!)}`] : []))].join(" ")),
    ].map((l) => `${pad}    ${l}`);
    const inner = (kids.get(b.id) ?? []).map((k) => out(k, `${pad}    `));
    const head = `${pad}${ident(b.kind)} ${quote(b.id)} ${props.join(" ")}`;
    return body.length || inner.length ? `${head} {\n${[...body, ...inner].join("\n")}\n${pad}}` : head;
  };
  return (kids.get(null) ?? []).map((b) => out(b, "")).join("\n") + "\n";
}

type Value = string | number | boolean | null;
type Node = { name: string; args: Value[]; props: Record<string, Value>; kids: Node[] };

// the part of KDL (v1 and v2) that ig/ files use
function parse(src: string): Node[] {
  let i = 0;
  const fail = (): never => { throw new Error(`not KDL at ${i}`); };
  const nl = (c: string) => c === "\n" || c === "\r" || c === "\u2028" || c === "\u2029" || c === "\u0085" || c === "\f";
  const blank = (c: string) => /[\t \u00a0\ufeff\u1680\u2000-\u200a\u202f\u205f\u3000]/.test(c);
  function comment(): boolean {
    if (src.startsWith("//", i)) {
      while (i < src.length && !nl(src[i])) i++;
      return true;
    }
    if (src.startsWith("/*", i)) {
      let depth = 0;
      do {
        if (src.startsWith("/*", i)) (depth++, (i += 2));
        else if (src.startsWith("*/", i)) (depth--, (i += 2));
        else if (i >= src.length) fail();
        else i++;
      } while (depth > 0);
      return true;
    }
    return false;
  }
  function gap() {
    for (;;) {
      if (blank(src[i] ?? "")) i++;
      else if (src[i] === "\\") {
        i++;
        while (blank(src[i] ?? "") || comment()) if (blank(src[i] ?? "")) i++;
        if (src[i] === "\r" && src[i + 1] === "\n") i += 2;
        else if (nl(src[i] ?? "")) i++;
        else if (i < src.length) fail();
      } else if (!comment()) return;
    }
  }
  function lines(body: string, raw: boolean): string {
    const ls = body.split(/\r\n|[\n\r\u2028\u2029\u0085\f]/);
    if (ls.length < 2 || ls[0].trim() || ls.at(-1)!.trim()) fail();
    const pad = ls.at(-1)!, inner = ls.slice(1, -1).map((l) => (l.trim() ? (l.startsWith(pad) ? l.slice(pad.length) : fail()) : ""));
    const s = inner.join("\n");
    return raw ? s : unescape(s);
  }
  function unescape(s: string): string {
    return s.replace(/\\(u\{[0-9a-fA-F]{1,6}\}|[\s\S])(\s*)/g, (_, e: string, ws: string) => {
      if (e.startsWith("u{")) return String.fromCodePoint(parseInt(e.slice(2, -1), 16)) + ws;
      const m: Record<string, string> = { n: "\n", r: "\r", t: "\t", "\\": "\\", '"': '"', "/": "/", b: "\b", f: "\f", s: " " };
      if (/\s/.test(e)) return "";
      return (m[e] ?? fail()) + ws;
    });
  }
  function quoted(): string {
    if (src.startsWith('"""', i)) {
      i += 3;
      const start = i;
      for (; !src.startsWith('"""', i); i++) {
        if (i >= src.length) fail();
        if (src[i] === "\\") i++;
      }
      const body = src.slice(start, i);
      i += 3;
      return lines(body, false);
    }
    i++;
    const start = i;
    for (; src[i] !== '"'; i++) {
      if (i >= src.length) fail();
      if (src[i] === "\\") i++;
    }
    const body = src.slice(start, i);
    i++;
    return unescape(body);
  }
  function raw(): string {
    if (src[i] === "r") i++;
    let n = 0;
    while (src[i] === "#") (n++, i++);
    const multi = src.startsWith('"""', i), open = multi ? 3 : 1;
    i += open;
    const close = (multi ? '"""' : '"') + "#".repeat(n), end = src.indexOf(close, i);
    if (end < 0) fail();
    const body = src.slice(i, end);
    i = end + close.length;
    return multi ? lines(body, true) : body;
  }
  const isRaw = () => (src[i] === "#" || (src[i] === "r" && (src[i + 1] === "#" || src[i + 1] === '"'))) && /^r?#*"/.test(src.slice(i, i + 80));
  function bare(): string {
    const start = i;
    while (i < src.length && !blank(src[i]) && !nl(src[i]) && !/[\\/(){}[\]";=#]/.test(src[i])) i++;
    if (i === start) fail();
    return src.slice(start, i);
  }
  function value(): Value {
    if (src[i] === "(") {
      const end = src.indexOf(")", i);
      if (end < 0) fail();
      i = end + 1;
    }
    if (src[i] === '"') return quoted();
    if (isRaw()) return raw();
    if (src[i] === "#") {
      i++;
      const w = bare();
      return w === "true" ? true : w === "false" ? false : w === "null" ? null : w === "inf" ? Infinity : w === "-inf" ? -Infinity : w === "nan" ? NaN : fail();
    }
    const w = bare();
    if (/^[+-]?\d/.test(w)) return Number(w.replace(/_/g, ""));
    return w === "true" ? true : w === "false" ? false : w === "null" ? null : w;
  }
  function node(): Node {
    const n = value();
    if (typeof n !== "string") fail();
    const out: Node = { name: n as string, args: [], props: {}, kids: [] };
    for (;;) {
      gap();
      const c = src[i];
      if (c === undefined || nl(c) || c === ";" || c === "}") return out;
      if (src.startsWith("/-", i)) {
        i += 2;
        gap();
        if (src[i] === "{") (i++, nodes(true), i++);
        else entry({ name: "", args: [], props: {}, kids: [] });
        continue;
      }
      if (c === "{") {
        i++;
        out.kids = nodes(true);
        i++;
        continue;
      }
      entry(out);
    }
  }
  function entry(n: Node) {
    const v = value();
    gap();
    if (src[i] === "=") {
      i++;
      gap();
      n.props[String(v)] = value();
    } else n.args.push(v);
  }
  function nodes(inBlock: boolean): Node[] {
    const out: Node[] = [];
    for (;;) {
      gap();
      const c = src[i];
      if (c === undefined) return inBlock ? fail() : out;
      if (nl(c) || c === ";") {
        i++;
        continue;
      }
      if (c === "}") return inBlock ? out : fail();
      if (src.startsWith("/-", i)) {
        i += 2;
        gap();
        node();
        continue;
      }
      out.push(node());
    }
  }
  return nodes(false);
}

export function fromKdl(text: string, kinds: KindView[]): Copy | null {
  let doc: Node[];
  try {
    doc = parse(text);
  } catch {
    return null;
  }
  const known = new Set([...kinds.map((k) => k.name), "box", "group"]);
  const boxes: CopyBox[] = [], raw: CopyArrow[] = [];
  const read = (n: Node, parent: string | null): boolean => {
    const kind = OLD[n.name] ?? n.name, id = n.args[0];
    if (!known.has(kind) || typeof id !== "string" || !id) return false;
    const props: Record<string, string> = {};
    for (const [k, v] of Object.entries(n.props)) if (typeof v === "string" && k !== "title" && k !== "slot") props[k] = v;
    const b: CopyBox = { id, parent, kind, title: typeof n.props.title === "string" ? n.props.title : id, props };
    boxes.push(b);
    for (const k of n.kids) {
      const a = ARROWS[k.name];
      if (a) {
        const to = k.args[0], s = (x: Value | undefined) => (typeof x === "string" && x ? x : undefined);
        if (typeof to === "string") raw.push({ from: id, to, kind: a, ...Object.fromEntries((["label", "note", "context"] as const).flatMap((p) => (s(k.props[p]) ? [[p, s(k.props[p])]] : []))) });
      } else if (k.name === "context") {
        if (typeof k.args[0] === "string") b.context = k.args[0];
      } else if (!["at", "badges", "pattern"].includes(k.name) && !read(k, id)) return false;
    }
    return true;
  };
  for (const n of doc) {
    if (n.name === "system") continue;
    if (!read(n, null)) return null;
  }
  if (!boxes.length) return null;
  const ids = new Set(boxes.map((b) => b.id));
  return { boxes, arrows: raw.filter((a) => ids.has(a.to) && a.to !== a.from) };
}

const snap = (v: number) => Math.round(v / GRID) * GRID;

function fresh(id: string, parent: string, root: string, taken: Set<string>): string {
  const slash = id.lastIndexOf("/"), tail = id.slice(slash + 1);
  const prefix = slash >= 0 && parent !== root ? `${parent.split("/")[0]}/` : "";
  let out = prefix + tail;
  const base = tail.replace(/-\d+$/, "") || tail;
  for (let n = 2; taken.has(out); n++) {
    const suffix = `-${n}`;
    out = prefix + base.slice(0, MAX_ID - suffix.length).replace(/-+$/, "") + suffix;
  }
  taken.add(out);
  return out;
}

const BOX = { w: 10 * GRID, h: 6 * GRID };

export function paste(c: Copy, parent: string, at: { x: number; y: number } | null, view: View, root: string): { batch: Edit; places: Record<string, Place>; ids: string[] } {
  const byId = new Map(view.boxes.map((b) => [b.id, b])), taken = new Set(view.boxes.map((b) => b.id));
  const ids = new Map<string, string>(), edits: Edit[] = [], places: Record<string, Place> = {}, tops: string[] = [];
  const beside = (b: CopyBox) => byId.has(b.id) && (byId.get(b.id)!.parent ?? root) === parent;
  for (const b of c.boxes) {
    const into = b.parent === null ? parent : ids.get(b.parent);
    if (into === undefined) continue;
    const id = fresh(b.id, into, root, taken);
    ids.set(b.id, id);
    if (b.parent === null) tops.push(b.id);
    const title = b.parent === null && beside(b) ? `${b.title.slice(0, MAX_TITLE - 5).trimEnd()} copy` : b.title;
    edits.push({ op: "add", parent: into, kind: b.kind, id, title, at: [], before: null, ...(Object.keys(b.props).length ? { props: { ...b.props } } : {}) });
    if (b.context) edits.push({ op: "set", id, key: "context", value: b.context });
    if (b.parent !== null && b.place) places[id] = { ...b.place };
    if (b.room) places[innerKey(id)] = { ...b.room };
  }
  for (const a of c.arrows) {
    const from = ids.get(a.from), to = ids.get(a.to);
    if (!from || !to) continue;
    edits.push({ op: "arrow", from, to, kind: a.kind, label: a.label ?? null });
    for (const k of ["note", "context"] as const) if (a[k]) edits.push({ op: "arrowset", from, to, key: k, value: a[k]! });
  }
  const top = c.boxes.filter((b) => b.parent === null && ids.has(b.id));
  // places from another parent's inside mean nothing here: those boxes go in a row beside the others
  const kept = (b: CopyBox) => !!b.place && (b.from === undefined || b.from === c.parent);
  const own = top.some(kept) && (at !== null || c.parent === parent || top.every((b) => !byId.has(b.id)));
  if (own || at) {
    const local: Record<string, Place> = {}, held = own ? top.filter(kept) : [];
    let x = Math.max(0, ...held.map((b) => b.place!.x + b.place!.w + 2 * GRID));
    const y = held[0]?.place!.y ?? 0;
    for (const b of top) {
      const kids = c.boxes.some((k) => k.parent === b.id), size = b.place ? { w: b.place.w, h: b.place.h } : kids ? { w: 2 * BOX.w, h: 2 * BOX.h } : BOX;
      if (held.includes(b)) local[b.id] = b.place!;
      else {
        local[b.id] = { x, y, ...size };
        x += size.w + 2 * GRID;
      }
    }
    const ps = Object.values(local), l = Math.min(...ps.map((p) => p.x)), t = Math.min(...ps.map((p) => p.y));
    const w = Math.max(...ps.map((p) => p.x + p.w)) - l, h = Math.max(...ps.map((p) => p.y + p.h)) - t;
    const next = top.some(beside);
    const dx = at ? snap(at.x - w / 2) - l : next ? w + 2 * GRID : 0, dy = at ? snap(at.y - h / 2) - t : 0;
    for (const b of top) places[ids.get(b.id)!] = { ...local[b.id], x: local[b.id].x + dx, y: local[b.id].y + dy };
  }
  return { batch: { op: "batch", edits }, places, ids: tops.map((id) => ids.get(id)!) };
}
