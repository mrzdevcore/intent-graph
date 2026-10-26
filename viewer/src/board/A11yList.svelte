<!-- The board for screen readers and the keyboard: a visually hidden tree mirroring `a11y()`.
     Up and Down move the engine selection, Enter renames, Delete removes. Claude's edits and
     check summaries are announced in a polite live region. -->
<script lang="ts">
  import type { A11yItem } from "./engine";

  let { items, message, onselect, onkey }: {
    items: A11yItem[];
    message: string;
    onselect: (id: string) => void;
    onkey: (key: string) => void;
  } = $props();

  const at = $derived(items.findIndex((i) => i.selected));

  function describe(i: A11yItem): string {
    const kind = i.kind[0].toUpperCase() + i.kind.slice(1);
    return [`${kind} ${i.text}`, i.highlighted && "changed by Claude", i.problem].filter(Boolean).join(", ");
  }

  function onkeydown(e: KeyboardEvent) {
    e.stopPropagation();
    const step = e.key === "ArrowDown" ? 1 : e.key === "ArrowUp" ? -1 : 0;
    if (step && items.length) {
      e.preventDefault();
      const i = at < 0 ? (step > 0 ? 0 : items.length - 1) : Math.min(Math.max(at + step, 0), items.length - 1);
      onselect(items[i].id);
    } else if (e.key === "Enter" || e.key === "Delete" || e.key === "Backspace") {
      e.preventDefault();
      onkey(e.key);
    }
  }
</script>

<div class="sr-only">
  <ul role="tree" aria-label="Board elements" tabindex="0" aria-activedescendant={at < 0 ? undefined : "a11y-" + at} {onkeydown}>
    {#each items as item, i (item.id)}
      <li role="treeitem" id={"a11y-" + i} aria-selected={item.selected ? "true" : "false"}>{describe(item)}</li>
    {/each}
  </ul>
  <div role="status" aria-live="polite">{message}</div>
</div>

<style>
  .sr-only { position: absolute; width: 1px; height: 1px; padding: 0; margin: -1px; overflow: hidden;
    clip: rect(0 0 0 0); white-space: nowrap; border: 0; }
</style>
