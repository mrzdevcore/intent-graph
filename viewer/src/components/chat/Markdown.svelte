<!-- Agent text as markdown: sanitized HTML (model/markdown.ts); a link click opens the system
     browser through `openUrl`, never the webview. Graph refs (`match`) become buttons that show the
     node in the graph (`onnode`). -->
<script lang="ts">
  import { linkNodes, renderMarkdown } from "../../model/markdown";

  let { text, openUrl, match, onnode }: {
    text: string;
    openUrl?: (url: string) => void;
    match?: (text: string, code?: boolean) => string | null;
    onnode?: (id: string) => void;
  } = $props();

  const html = $derived(renderMarkdown(text));
  let el: HTMLDivElement;

  $effect(() => {
    el.innerHTML = html; // sanitized by renderMarkdown
    if (match && onnode) linkNodes(el, match);
  });

  function click(e: MouseEvent) {
    const t = e.target as Element;
    const node = t.closest?.("button[data-node]") as HTMLElement | null;
    if (node) return onnode?.(node.dataset.node!);
    const a = t.closest?.("a");
    if (!a) return;
    e.preventDefault();
    const href = a.getAttribute("href") ?? "";
    if (/^https?:\/\//i.test(href)) openUrl?.(href);
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="md" bind:this={el} onclick={click}></div>
