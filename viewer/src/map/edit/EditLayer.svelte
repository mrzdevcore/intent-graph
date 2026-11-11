<!-- The open popover of an EditSession: the arrow's rubber band and kind chips, the box menu, rename, move picker. -->
<script lang="ts">
  import type { ArrowKindView, KindView, MapBox } from "../types";
  import ArrowChip from "./ArrowChip.svelte";
  import ArrowDraw from "./ArrowDraw.svelte";
  import { arrowKindNames } from "./arrows";
  import BoxMenu from "./BoxMenu.svelte";
  import MovePicker from "./MovePicker.svelte";
  import RenameBox from "./RenameBox.svelte";
  import { parentsFor } from "./menu";
  import type { EditSession } from "./session.svelte";

  let { s, arrangeable, boxes, kinds, arrowKinds, root }: { s: EditSession; arrangeable: Set<string>; boxes: MapBox[]; kinds: KindView[]; arrowKinds?: ArrowKindView[]; root: string } = $props();
  const ui = $derived(s.ui);
  const band = $derived(s.band);
  const chip = $derived(s.chip);
</script>

{#if band}<ArrowDraw line={band} />{/if}
{#if chip && s.made}
  <ArrowChip kind={s.made.kind} kinds={arrowKindNames(arrowKinds)} x={chip.x} y={chip.y} open={s.made.open} ontoggle={(o) => s.chipOpen(o)} onpick={(k) => s.rekind(k)} />
{/if}
{#if ui}
  {@const id = ui.id}
  {#if ui.t === "menu"}
    <BoxMenu x={ui.x} y={ui.y} title={s.titleOf(id)} group={arrangeable.has(id)} onpick={(a) => s.act(a, id, { x: ui.x, y: ui.y })} onclose={() => s.shut()} />
  {:else if ui.t === "rename"}
    <RenameBox value={s.titleOf(id)} x={ui.x} y={ui.y} w={s.width(id)} onclose={() => s.shut()} onleave={() => s.shut(false)}
      onsubmit={(v) => s.settle((id) => v && v !== s.titleOf(id) ? { op: "set", id, key: "title", value: v } : null)} />
  {:else}
    <MovePicker title={s.titleOf(id)} x={ui.x} y={ui.y} onclose={() => s.shut()}
      options={parentsFor(boxes, kinds, id, root).map((b) => ({ id: b.id, title: b.title.value, kind: b.kind.value }))}
      onpick={(parent) => s.settle((id) => ({ op: "move", id, parent, before: null }))} />
  {/if}
{/if}
