// The plan being applied or checked (Task 5): its title, steps, volume and findings, collapsible.

import { LitElement, html } from "lit";
import { customElement, property } from "lit/decorators.js";
import type { Note, ViewData } from "../data";
import { noteLine } from "../model/notes";
import { findingItem } from "./shared";

const pairs = (ps: [string, string][]) => (ps && ps.length ? ps.map((p) => p[0] + " → " + p[1]).join(", ") : "none");
const counts = (o: Record<string, number>) => {
  const ks = Object.keys(o || {});
  return ks.length ? ks.map((k) => k + " " + o[k]).join(", ") : "none";
};

@customElement("ig-plan-panel")
export class IgPlanPanel extends LitElement {
  @property({ attribute: false }) data!: ViewData;
  /** Ids of the nodes in the graph, to pin plan findings to. */
  @property({ attribute: false }) known: Record<string, unknown> = {};
  @property({ attribute: false }) notes: Note[] = [];

  createRenderRoot() {
    return this;
  }

  render() {
    const p = this.data.plan;
    if (!p) return null;
    const v = p.volume;
    return html`<details open>
      <summary><h2>Plan${p.done ? " (done)" : ""}</h2></summary>
      <p><strong>${p.title}</strong></p>
      ${p.steps.length ? html`<ol>${p.steps.map((t) => html`<li>${t}</li>`)}</ol>` : null}
      ${v
        ? html`<ul>${[
            "new files: " + v.newFiles, "new nodes: " + counts(v.newNodes), "changed nodes: " + v.changedNodes,
            "removed nodes: " + v.removedNodes, "new systems: " + (v.newSystems.length ? v.newSystems.join(", ") : "none"),
            "new cross-layer links: " + pairs(v.newCrossLayer), "new cross-lane links: " + pairs(v.newCrossLane),
            "blast radius: " + v.blastRadius.total + " (" + counts(v.blastRadius.byLayer) + ")",
          ].map((t) => html`<li class="loc">${t}</li>`)}</ul>`
        : null}
      <h2>Plan findings (${p.findings.length})</h2>
      ${p.findings.length
        ? html`<ul>${p.findings.map((f) => findingItem(this, this.data, { ...f, pinned: f.nodeId && this.known[f.nodeId] ? f.nodeId : null }))}</ul>`
        : html`<p class="loc">None.</p>`}
      ${this.notes.length
        ? html`<h2>Notes (${this.notes.filter((n) => !n.done).length} open)</h2>
          <ul class="notes">${this.notes.map((n) => html`<li class=${n.done ? "done" : ""}>
            <strong>${n.id}</strong> ${n.done ? "✓ " : ""}${noteLine(n)}${n.reply ? html`<div class="loc">↳ ${n.reply}</div>` : null}</li>`)}</ul>`
        : null}
    </details>`;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    "ig-plan-panel": IgPlanPanel;
  }
}
