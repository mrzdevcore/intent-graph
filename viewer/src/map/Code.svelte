<!-- A snippet's lines, numbered and coloured (highlight.ts), for a <pre> its parent styles. Text only. -->
<script lang="ts">
  import type { Snippet } from "./types";
  import { tokens } from "./highlight";

  let { source }: { source: Snippet } = $props();
  const lines = $derived(tokens(source.file, source.text));
</script>

{#each lines as line, i}<span class="ln">{source.line + i}</span>{#each line as t}<span style:color={t.color} style:font-style={t.italic ? "italic" : null}>{t.text}</span>{/each}{"\n"}{/each}{#if source.cut}<span class="ln">…</span>{source.cut} more lines{/if}

<style>
  .ln { display: inline-block; width: 3.5em; padding-right: 1em; text-align: right; color: var(--map-edge); user-select: none; }
</style>
