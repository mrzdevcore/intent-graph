<!-- The tool island, top centre: hand, select, box, arrow, frame, the kind-first form (K), then Arrange. A double-click locks a tool. -->
<script lang="ts">
  import { ArrowRight, Frame, Hand, MousePointer2, Shapes, Square, WandSparkles } from "lucide-static";
  import type { Tool } from "./tool.svelte";

  let { tool, locked, onpick, onkind, onarrange }: {
    tool: Tool; locked: boolean; onpick: (t: Tool, lock: boolean) => void; onkind: (e: MouseEvent) => void; onarrange: () => void;
  } = $props();
  const tools: { id: Tool; label: string; key: string; icon: string }[] = [
    { id: "hand", label: "Hand", key: "H", icon: Hand },
    { id: "select", label: "Select", key: "V", icon: MousePointer2 },
    { id: "box", label: "Box", key: "B", icon: Square },
    { id: "arrow", label: "Arrow", key: "A", icon: ArrowRight },
    { id: "frame", label: "Frame", key: "F", icon: Frame },
  ];
  const mask = (svg: string) => `url("data:image/svg+xml,${encodeURIComponent(svg)}")`;
</script>

<div class="island" role="toolbar" aria-label="Edit the map">
  {#each tools as t (t.id)}
    <button type="button" aria-label={t.label} title="{t.label} — {t.key}{t.id === 'box' || t.id === 'arrow' ? ' (double-click to keep it)' : ''}"
      aria-pressed={tool === t.id} data-locked={tool === t.id && locked ? "" : undefined}
      onclick={() => onpick(t.id, false)} ondblclick={() => onpick(t.id, true)}>
      <i style:mask-image={mask(t.icon)} aria-hidden="true"></i><kbd>{t.key}</kbd>
    </button>
  {/each}
  <button type="button" aria-label="Add a box by kind" title="Add a box by kind — K" onclick={onkind}>
    <i style:mask-image={mask(Shapes)} aria-hidden="true"></i><kbd>K</kbd>
  </button>
  <span class="sep" aria-hidden="true"></span>
  <button type="button" aria-label="Arrange" title="Arrange (⌥⇧F)" onclick={onarrange}><i style:mask-image={mask(WandSparkles)} aria-hidden="true"></i></button>
</div>

<style>
  .island { position: absolute; z-index: 4; left: 50%; top: var(--space-3); transform: translateX(-50%); display: flex; align-items: center; gap: var(--space-0-5);
    padding: var(--space-1); background: var(--map-surface); border: 1px solid var(--map-line); border-radius: var(--radius-xl); box-shadow: 0 0.375rem 1.25rem var(--map-shadow);
    max-width: calc(100% - 2rem); box-sizing: border-box; overflow-x: auto; }
  button { position: relative; flex: none; width: 2.25rem; height: 2.25rem; display: inline-flex; align-items: center; justify-content: center; padding: 0;
    border: 1px solid transparent; border-radius: var(--radius-lg); background: none; color: var(--map-fg-2); cursor: pointer; }
  button:hover { background: var(--map-sunken); }
  button[aria-pressed="true"] { background: color-mix(in srgb, var(--map-trace) 18%, var(--map-surface)); border-color: var(--map-trace); color: var(--map-fg); }
  button[data-locked]::after { content: ""; position: absolute; left: 0.25rem; top: 0.25rem; width: 0.3125rem; height: 0.3125rem; border-radius: 50%; background: var(--map-trace); }
  button:focus-visible { outline: 2px solid var(--map-trace); outline-offset: 1px; }
  i { width: 1.125rem; height: 1.125rem; background: currentColor; mask-repeat: no-repeat; mask-position: center; mask-size: contain; }
  kbd { position: absolute; right: 0.1875rem; bottom: 0.0625rem; font: 500 var(--text-2xs)/1 var(--font-sans); color: var(--map-faint); }
  .sep { flex: none; width: 1px; height: 1.5rem; margin: 0 var(--space-1); background: var(--map-line); }
  @container map (max-width: 80rem) { .island { top: 3.75rem; } }
</style>
