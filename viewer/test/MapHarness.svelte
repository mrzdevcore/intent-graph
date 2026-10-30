<!-- Test only: a Map that takes the views its edits return, as App does. -->
<script lang="ts">
  import Map from "../src/map/Map.svelte";
  import type { EditApi } from "../src/api";
  import type { View } from "../src/map/types";
  let { start, edit, size }: { start: View; edit: EditApi; size: { w: number; h: number } } = $props();
  // svelte-ignore state_referenced_locally
  let view = $state.raw(start);
</script>

<Map {view} {edit} {size} onview={(v) => (view = v)} />
