<!-- The rubber band of an arrow being drawn: dashed gold, from a box's side to the pointer or the side that catches it. -->
<script lang="ts">
  let { line }: { line: { x1: number; y1: number; x2: number; y2: number } } = $props();
</script>

<svg aria-hidden="true" data-band><line x1={line.x1} y1={line.y1} x2={line.x2} y2={line.y2} /><circle cx={line.x2} cy={line.y2} r="4" /></svg>

<style>
  svg { position: absolute; inset: 0; width: 100%; height: 100%; pointer-events: none; z-index: 4; overflow: visible; }
  line { stroke: var(--map-trace); stroke-width: 2; stroke-dasharray: 6 4; }
  circle { fill: var(--map-trace); }
</style>
