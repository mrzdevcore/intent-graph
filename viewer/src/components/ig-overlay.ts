// The wide view over the page: a function's code, or a call chain (Mermaid diagram, offline, beside
// its clickable step list) with Mermaid and PlantUML text to copy. Escape or Close removes it.

import { LitElement, html } from "lit";
import { customElement, property } from "lit/decorators.js";
import mermaid from "mermaid";
import { copyButton } from "./shared";

/** A code row: op, line number, text. */
export type CodeRow = [op: string, line: number | null, text: string];

export function codeBlock(rows: CodeRow[], wide = false) {
  return html`<pre class="code${wide ? " wide" : ""}">${rows.map(
    (r) => html`<div class="cl${r[0] === "+" ? " add" : r[0] === "-" ? " del" : ""}"><span class="ln">${String(r[1] || "")}</span><span class="op">${r[0]}</span><span>${r[2]}</span></div>`,
  )}</pre>`;
}

/** The numbered, clickable step list; a "+N or more" row when the chain was cut. */
export function stepList(rows: { n: number; text: string; target: string }[], pick: (target: string) => void, truncated = 0) {
  return html`<ol class="steps">${rows.map((r) => html`<li class="f" @click=${() => pick(r.target)}>${r.text}</li>`)}${truncated
    ? html`<li class="loc more">+${truncated} or more (lower the depth)</li>`
    : null}</ol>`;
}

let mermaidReady = false;
let seqCount = 0;

function renderSequence(container: HTMLElement, text: string) {
  if (!mermaidReady) {
    const dark = getComputedStyle(document.documentElement).getPropertyValue("--bg").trim().toLowerCase() < "#888";
    mermaid.initialize({ startOnLoad: false, securityLevel: "strict", theme: dark ? "dark" : "default", sequence: { useMaxWidth: false } });
    mermaidReady = true;
  }
  mermaid
    .render("igseq" + seqCount++, text)
    .then((r) => (container.innerHTML = r.svg))
    .catch((err) => {
      const pre = document.createElement("pre");
      pre.textContent = text + "\n\n" + err;
      container.appendChild(pre);
    });
}

@customElement("ig-overlay")
export class IgOverlay extends LitElement {
  @property() heading = "";
  @property({ attribute: false }) code?: CodeRow[];
  @property({ attribute: false }) mermaid?: string;
  @property({ attribute: false }) plantUml?: string;
  @property({ attribute: false }) steps?: { n: number; text: string; target: string }[];
  @property({ attribute: false }) truncated = 0;
  @property({ attribute: false }) onPick?: (target: string) => void;

  private esc = (e: KeyboardEvent) => e.key === "Escape" && this.close();

  createRenderRoot() {
    return this;
  }
  connectedCallback() {
    super.connectedCallback();
    document.addEventListener("keydown", this.esc);
  }
  disconnectedCallback() {
    super.disconnectedCallback();
    document.removeEventListener("keydown", this.esc);
  }
  close() {
    this.remove();
  }

  render() {
    const copies = this.mermaid ? [copyButton("Copy Mermaid", this.mermaid), copyButton("Copy PlantUML", this.plantUml ?? "")] : [];
    return html`<div class="overlay" role="dialog" aria-label=${this.heading}>
      <div class="overlay-bar"><strong>${this.heading}</strong><span class="btns">${copies}<button @click=${() => this.close()}>Close</button></span></div>
      ${this.code
        ? codeBlock(this.code, true)
        : html`<div class="chain"><div class="seq"></div>${this.steps
            ? stepList(this.steps, (t) => { this.close(); this.onPick?.(t); }, this.truncated)
            : null}</div>`}
    </div>`;
  }

  firstUpdated() {
    if (this.mermaid) renderSequence(this.querySelector(".seq")!, this.mermaid);
  }
}

/** Opens an overlay on top of the page. */
export function openOverlay(o: Partial<Pick<IgOverlay, "code" | "mermaid" | "plantUml" | "steps" | "truncated" | "onPick">> & { heading: string }) {
  document.body.appendChild(Object.assign(new IgOverlay(), o));
}

declare global {
  interface HTMLElementTagNameMap {
    "ig-overlay": IgOverlay;
  }
}
