<!-- The wide view over the page: a function's code, or a call chain (Mermaid diagram, offline, beside
     its clickable step list) with Mermaid and PlantUML text to copy. Escape or Close removes it. -->
<script module lang="ts">
  import { mount, unmount } from "svelte";
  import mermaid from "mermaid";
  import Overlay from "./Overlay.svelte";
  import { copyButton } from "./shared.svelte";

  export { codeBlock, stepList };

  /** A code row: op, line number, text. */
  export type CodeRow = [op: string, line: number | null, text: string];
  export type Step = { n: number; text: string; target: string };

  type Props = {
    heading: string;
    code?: CodeRow[];
    mermaid?: string;
    plantUml?: string;
    steps?: Step[];
    truncated?: number;
    onPick?: (target: string) => void;
  };

  /** Opens an overlay on top of the page. */
  export function openOverlay(o: Props) {
    const inst: Record<string, unknown> = mount(Overlay, { target: document.body, props: { ...o, close: () => unmount(inst) } });
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
</script>

<script lang="ts">
  let { heading, code, mermaid: mm, plantUml, steps, truncated = 0, onPick, close }: Props & { close: () => void } = $props();
  let seq = $state<HTMLElement>();

  $effect(() => {
    if (mm && seq) renderSequence(seq, mm);
  });
</script>

<svelte:document onkeydown={(e) => e.key === "Escape" && close()} />

{#snippet codeBlock(rows: CodeRow[], wide = false)}
  <pre class="code{wide ? ' wide' : ''}">{#each rows as r}<div class="cl{r[0] === '+' ? ' add' : r[0] === '-' ? ' del' : ''}"><span class="ln">{String(r[1] || "")}</span><span class="op">{r[0]}</span><span>{r[2]}</span></div>{/each}</pre>
{/snippet}

<!-- The numbered, clickable step list; a "+N or more" row when the chain was cut. -->
{#snippet stepList(rows: Step[], pick: (target: string) => void, truncated = 0)}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
  <ol class="steps">{#each rows as r}<li class="f" onclick={() => pick(r.target)}>{r.text}</li>{/each}{#if truncated}<li class="loc more">+{truncated} or more (lower the depth)</li>{/if}</ol>
{/snippet}

<div class="overlay" role="dialog" aria-label={heading}>
  <div class="overlay-bar"><strong>{heading}</strong><span class="btns">{#if mm}{@render copyButton("Copy Mermaid", mm)}{@render copyButton("Copy PlantUML", plantUml ?? "")}{/if}<button onclick={close}>Close</button></span></div>
  {#if code}
    {@render codeBlock(code, true)}
  {:else}
    <div class="chain"><div class="seq" bind:this={seq}></div>{#if steps}{@render stepList(steps, (t) => { close(); onPick?.(t); }, truncated)}{/if}</div>
  {/if}
</div>
