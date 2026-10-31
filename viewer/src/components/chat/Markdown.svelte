<!-- Agent text as markdown: sanitized HTML (model/markdown.ts); a link click opens the system
     browser through `openUrl`, never the webview. -->
<script lang="ts">
  import { renderMarkdown } from "../../model/markdown";

  let { text, openUrl }: { text: string; openUrl?: (url: string) => void } = $props();

  const html = $derived(renderMarkdown(text));
  let el: HTMLDivElement;

  $effect(() => {
    el.innerHTML = html; // sanitized by renderMarkdown
  });

  function click(e: MouseEvent) {
    const a = (e.target as Element).closest?.("a");
    if (!a) return;
    e.preventDefault();
    const href = a.getAttribute("href") ?? "";
    if (/^https?:\/\//i.test(href)) openUrl?.(href);
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="md" bind:this={el} onclick={click}></div>
