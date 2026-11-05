<script lang="ts">
  import Map from "../src/map/Map.svelte";
  import type { EditApi } from "../src/api";
  import type { View } from "../src/map/types";
  import type { Show } from "../src/model/show";
  let { view, edit, size }: { view: View; edit?: EditApi; size: { w: number; h: number } } = $props();
  let show = $state.raw<Show | null>(null);
  export const say = (s: Show) => (show = s);
</script>

<Map {view} {edit} {size} {show} />
