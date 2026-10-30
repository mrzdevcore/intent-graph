<!-- The open popover of an EditSession: the arrow's rubber band and kind chips, the box menu, rename, move picker. -->
<script lang="ts">
  import type { MapBox } from "../types";
  import ArrowDraw from "./ArrowDraw.svelte";
  import BoxMenu from "./BoxMenu.svelte";
  import MovePicker from "./MovePicker.svelte";
  import RenameBox from "./RenameBox.svelte";
  import { parentsFor } from "./menu";
  import type { EditSession } from "./session.svelte";

  let { s, boxes, root }: { s: EditSession; boxes: MapBox[]; root: string } = $props();
  const ui = $derived(s.ui);
</script>

{#if s.link || ui?.t === "arrow"}
  <ArrowDraw line={s.band} pick={ui?.t === "arrow" ? { from: s.titleOf(ui.from), to: s.titleOf(ui.to), x: ui.x, y: ui.y } : undefined}
    onpick={(kind, label) => s.draw(kind, label)} onclose={() => s.shut()} />
{/if}
{#if ui && ui.t !== "arrow"}
  {@const id = ui.id}
  {#if ui.t === "menu"}
    <BoxMenu x={ui.x} y={ui.y} title={s.titleOf(id)} onpick={(a) => s.act(a, id, { x: ui.x, y: ui.y })} onclose={() => s.shut()} />
  {:else if ui.t === "rename"}
    <RenameBox value={s.titleOf(id)} x={ui.x} y={ui.y} w={s.width(id)} onclose={() => s.shut()} onleave={() => s.shut(false)}
      onsubmit={(v) => s.settle((id) => v && v !== s.titleOf(id) ? { op: "set", id, key: "title", value: v } : null)} />
  {:else}
    <MovePicker title={s.titleOf(id)} x={ui.x} y={ui.y} onclose={() => s.shut()}
      options={parentsFor(boxes, id, root).map((b) => ({ id: b.id, title: b.title.value, kind: b.kind.value }))}
      onpick={(parent) => s.settle((id) => ({ op: "move", id, parent, before: null }))} />
  {/if}
{/if}
