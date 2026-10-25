// The side panel: the overview (notices, plan, intent changes, findings, intents), a group, a node
// (code diff, calls down, findings, edges) or, in Blueprint, the journey list.

import { LitElement, html } from "lit";
import { customElement, property } from "lit/decorators.js";
import { keyed } from "lit/directives/keyed.js";
import type { Note, ViewEdge, ViewFinding, ViewNode } from "../data";
import { ancestors, type Index } from "../model/graph";
import { journeyName, journeys } from "../model/blueprint";
import { callChain, chainToMermaid, chainToPlantUml, stepRows } from "../model/callchain";
import { codeBlock, openOverlay, stepList, type CodeRow } from "./ig-overlay";
import "./ig-plan-panel";
import { copyButton, editorLink, emit, findingItem } from "./shared";

export type SideView = { kind: "overview" } | { kind: "journeys" } | { kind: "group"; id: string } | { kind: "node"; id: string };

@customElement("ig-side-panel")
export class IgSidePanel extends LitElement {
  @property({ attribute: false }) ix!: Index;
  @property({ attribute: false }) view: SideView = { kind: "overview" };
  @property({ attribute: false }) mode = "";
  @property({ attribute: false }) focusId: string | null = null;
  @property({ attribute: false }) focusDepth = 1;
  @property({ attribute: false }) journey: string | null = null;
  @property({ attribute: false }) uiOnly = false;
  /** Plan notes from `ig serve`; empty when not live. */
  @property({ attribute: false }) notes: Note[] = [];

  createRenderRoot() {
    return this;
  }

  render() {
    const v = this.view;
    const body =
      v.kind === "journeys" ? this.journeyList()
      : v.kind === "group" ? this.group(v.id)
      : v.kind === "node" ? this.node(v.id)
      : this.overview();
    // A fresh DOM per view, so buttons ("Copied") start over.
    return keyed(v.kind + ":" + ("id" in v ? v.id : ""), body);
  }

  private findings(fs: ViewFinding[]) {
    return html`<ul>${fs.map((f) => findingItem(this, this.ix.D, f))}</ul>`;
  }

  private backLink() {
    const back = (e: Event) => {
      e.preventDefault();
      emit(this, "ig-overview");
    };
    return html`<p><a href="#" @click=${back}>← overview</a></p>`;
  }

  private overview() {
    const D = this.ix.D;
    const mark = { added: "+", changed: "~", removed: "-", unchanged: "" };
    return html`
      ${D.notices.map((n) => html`<p class="notice">${n}</p>`)}
      ${D.plan ? html`<ig-plan-panel .data=${D} .known=${this.ix.byId} .notes=${this.notes}></ig-plan-panel>` : null}
      ${D.intentChanges.length
        ? html`<h2>Intent changes</h2><ul>${D.intentChanges.map((c) => html`<li class=${c.status}>${mark[c.status] + " " + c.id + ": " + c.text}</li>`)}</ul>`
        : null}
      <h2>Findings (${D.findings.length})</h2>
      ${D.findings.length ? this.findings(D.findings) : html`<p class="loc">None.</p>`}
      ${D.intents.length
        ? html`<h2>Intents</h2><ul>${D.intents.map((i) => {
            const go = () => {
              const t = i.implementedBy[0] || i.testedBy[0];
              if (t) emit(this, "ig-focus", t);
            };
            return html`<li class="f" @click=${go}><strong>${i.id + " "}</strong><span>${i.text}</span>
              <div class="loc">${(i.implementedBy.length ? "implemented" : "not implemented") + " · " + (i.testedBy.length ? "tested" : "untested")}</div></li>`;
          })}</ul>`
        : null}`;
  }

  private group(gid: string) {
    const { ix } = this;
    const g = ix.groups[gid];
    if (!g) return null;
    const fs = ix.D.findings.filter((f) => {
      const n = f.pinned && ix.byId[f.pinned];
      return n && ancestors(ix, ix.treeOf[n.file]).indexOf(g) >= 0;
    });
    return html`${this.backLink()}<h2>${g.layer ? "layer" : "directory"}</h2>
      <p><strong>${g.label}</strong> <span class="loc">${g.count} modules</span></p>
      ${fs.length ? html`<h2>Findings</h2>${this.findings(fs)}` : null}`;
  }

  private node(id: string) {
    const { ix } = this;
    const n = ix.byId[id];
    if (!n) return this.overview();
    const D = ix.D;
    const fs = n.module ? D.findings.filter((f) => f.file === n.file) : ix.findingsByNode[id] || [];
    const ins = D.edges.filter((e) => e.to === id && e.from !== id);
    const outs = D.edges.filter((e) => e.from === id && e.to !== id);
    return html`${this.backLink()}
      <h2>${n.kind + (n.layer ? " · " + n.layer : "")}</h2>
      <p><strong>${n.name}</strong> <span class=${n.status}>${n.status}</span>${n.planned ? html`<span class="tag">planned</span>` : null}</p>
      ${n.planned
        ? null
        : html`<div class="loc">${editorLink(D, n.file, n.line)}</div><div class="loc">complexity ${n.complexity} · ${n.loc} lines</div>`}
      <div class="btns">
        <button @click=${() => emit(this, "ig-focus", id)}>Focus</button>
        ${this.mode === "focus" && this.focusId === id
          ? html`<button @click=${() => emit(this, "ig-depth", id)}>${this.focusDepth === 1 ? "Depth 2" : "Depth 1"}</button>`
          : null}
        ${this.mode !== "architecture" ? html`<button @click=${() => emit(this, "ig-arch", id)}>Show in architecture</button>` : null}
      </div>
      ${n.intents.length ? html`<p>${n.intents.map((t) => html`<span class="tag">${t}</span>`)}</p>` : null}
      ${n.planned ? html`<p class="loc">planned, not written yet</p>` : this.code(n)}
      ${this.callsDown(id)}
      ${fs.length ? html`<h2>Findings</h2>${this.findings(fs)}` : null}
      ${this.edgeList("Used by", ins, "from")}
      ${this.edgeList("Uses", outs, "to")}`;
  }

  /** The function's code: its diff when it changed, its text when it is new or removed. */
  private code(n: ViewNode) {
    const rows: CodeRow[] = [];
    if (n.diff) n.diff.forEach((d) => rows.push([d[0], d[0] === "-" ? d[1] : d[2], d[3]]));
    else if (n.code) n.code.text.split("\n").forEach((t, i) => rows.push([n.code!.op, n.code!.line + i, t]));
    if (!rows.length) return null;
    const title = n.diff ? "Code (before → after)" : n.code!.op === "+" ? "Code (new)" : "Code (removed)";
    const wide = () => openOverlay({ heading: n.name + " · " + n.file + ":" + n.line, code: rows });
    return html`<h2>${title}</h2><div class="btns"><button @click=${wide}>Open wide</button></div>${codeBlock(rows)}`;
  }

  private chainDepth = 8;

  private callsDown(id: string) {
    const c = callChain(this.ix, id, { depth: this.chainDepth });
    const stepper = html`<span class="loc">depth ${this.chainDepth}</span>
      <button ?disabled=${this.chainDepth <= 1} @click=${() => this.setDepth(-1)}>−</button>
      <button ?disabled=${this.chainDepth >= 10} @click=${() => this.setDepth(1)}>+</button>`;
    if (!c.steps.length) return html`<h2>Calls down</h2><p class="loc">No calls from here</p><div class="btns">${stepper}</div>`;
    const mm = chainToMermaid(c);
    const pu = chainToPlantUml(c);
    const rows = stepRows(c);
    const open = (t: string) => emit(this, "ig-focus", t);
    const show = () =>
      openOverlay({ heading: "Calls down from " + this.ix.byId[id].name, mermaid: mm, plantUml: pu, steps: rows, truncated: c.truncated, onPick: open });
    return html`<h2>Calls down (${c.steps.length} steps)</h2>
      <div class="btns"><button @click=${show}>Show diagram</button>${copyButton("Copy Mermaid", mm)}${copyButton("Copy PlantUML", pu)}${stepper}</div>
      ${stepList(rows, open, c.truncated)}`;
  }

  private setDepth(d: number) {
    this.chainDepth = Math.min(10, Math.max(1, this.chainDepth + d));
    this.requestUpdate();
  }

  private edgeList(title: string, list: ViewEdge[], key: "from" | "to") {
    if (!list.length) return null;
    return html`<h2>${title} (${list.length})</h2><ul>${list.slice(0, 50).map((e) => {
      const o = this.ix.byId[e[key]] || { name: e[key], file: "" };
      const violation = e.violation ? html`<span class="sev error"> violation</span>` : null;
      const planned = e.planned ? html`<span class="tag">planned</span>` : null;
      return html`<li class="f" @click=${() => emit(this, "ig-focus", e[key])}><span>${e.kind + " "}</span><strong>${o.name}</strong>${violation}${planned}<div class="loc">${o.file}</div></li>`;
    })}</ul>`;
  }

  private journeyList() {
    const D = this.ix.D;
    if (!D.lanes.length)
      return html`<h2>Journeys</h2><p class="loc">Add a blueprint section to ig.yaml (lanes, http, systems) to see journeys.</p>`;
    const js = journeys(this.ix);
    if (!js.length) return html`<h2>Journeys</h2><p class="loc">No module is in the first lane (${D.lanes[0]}).</p>`;
    let app: string | null = null;
    return html`<h2>Journeys</h2>
      <label class="loc"><input type="checkbox" .checked=${this.uiOnly}
        @change=${(ev: Event) => emit(this, "ig-ui-only", (ev.target as HTMLInputElement).checked)}> show UI-only components</label>
      ${js.map((n) => {
        const jn = journeyName(n.file);
        const head = jn.app !== app ? html`<h2>${jn.app || "pages"}</h2>` : null;
        app = jn.app;
        return html`${head}<ul><li class="f${n.id === this.journey ? " current" : ""}" @click=${() => emit(this, "ig-journey", n.id)}><strong>${jn.route}</strong></li></ul>`;
      })}`;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    "ig-side-panel": IgSidePanel;
  }
}
