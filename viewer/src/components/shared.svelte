<!-- Bits the panels share: editor links, finding rows and copy buttons. -->
<script module lang="ts">
  import CopyButton from "./CopyButton.svelte";
  import type { ViewData, ViewFinding } from "../data";

  export { editorLink, findingItem, copyButton };

  /** Editor links: URL schemes become links; javascript:/data:/vbscript: never do. */
  function editorTarget(D: ViewData, file: string, line: number) {
    const abs = D.root.replace(/\/$/, "") + "/" + file;
    const target = D.editor.split("{file}").join(abs).split("{line}").join(String(line || 1));
    const m = /^([a-z][a-z0-9+.-]*):/i.exec(target);
    return { target, link: !!m && ["javascript", "data", "vbscript"].indexOf(m[1].toLowerCase()) < 0 };
  }
</script>

{#snippet editorLink(D: ViewData, file: string, line: number)}
  {@const e = editorTarget(D, file, line)}
  {#if e.link}<a href={e.target}>{file}:{line}</a>{:else}<code title={e.target}>{file}:{line}</code>{/if}
{/snippet}

<!-- A finding; clicking it focuses the node it is pinned to. -->
{#snippet findingItem(D: ViewData, f: ViewFinding, focus: (id: string) => void)}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
  <li class="f" onclick={() => f.pinned && focus(f.pinned)}>
    <span class="sev {f.severity}">{f.severity}</span><span>{f.message}</span>
    <div class="loc">{f.ruleId} · {@render editorLink(D, f.file, f.line)}</div>
  </li>
{/snippet}

{#snippet copyButton(label: string, text: string)}
  <CopyButton {label} {text} />
{/snippet}
