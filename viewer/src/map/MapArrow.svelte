<!-- An arrow: polyline in the parent's inner units, a head, and a label pill that cuts the line
     at the middle of the longest segment (hidden at far detail). -->
<script lang="ts" module>
  let n = 0;
</script>
<script lang="ts">
  import type { arrowLook as AL } from "./looks";

  let { points, arrowLook, detail = "near" }: {
    points: [number, number][]; arrowLook: ReturnType<typeof AL>; detail?: "far" | "mid" | "near";
  } = $props();

  const id = `ig-head-${n++}`;
  const mid = $derived.by(() => {
    let best = 0, at: [number, number] = points[0] ?? [0, 0];
    for (let i = 1; i < points.length; i++) {
      const [a, b] = [points[i - 1], points[i]], len = Math.hypot(b[0] - a[0], b[1] - a[1]);
      if (len > best) { best = len; at = [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2]; }
    }
    return at;
  });
  const w = $derived((arrowLook.label?.length ?? 0) * 6.5 + 16);
  const TEXT = { neutral: "#23211e", warn: "#6b4200", bad: "#9b2c1f", down: "#4a4640" };
</script>

<svg style="position:absolute;left:0;top:0;overflow:visible;pointer-events:none;fill:none" width="1" height="1" aria-hidden="true">
  <defs>
    <marker {id} orient="auto" markerUnits="userSpaceOnUse" markerWidth="12" markerHeight="12" refX={arrowLook.head === "filled" ? 4 : 6} refY="4" overflow="visible">
      {#if arrowLook.head === "filled"}<path d="M0 0 L8 4 L0 8 Z" fill={arrowLook.color} />
      {:else if arrowLook.head === "open"}<path d="M0 0 L6 4 L0 8" fill="none" stroke={arrowLook.color} stroke-width="1.5" />
      {:else}<path d="M0 0 L8 4 L0 8 Z" fill="#faf8f3" stroke={arrowLook.color} stroke-width="1.5" />{/if}
    </marker>
  </defs>
  <polyline points={points.map((p) => p.join(",")).join(" ")} stroke={arrowLook.color} stroke-width={arrowLook.width}
    stroke-dasharray={arrowLook.dash ?? undefined} stroke-linecap="round" stroke-linejoin="round" marker-end="url(#{id})" />
  {#if arrowLook.label && detail !== "far"}
    <rect x={mid[0] - w / 2} y={mid[1] - 10} width={w} height="20" rx="10" fill="#faf8f3" stroke={arrowLook.color} stroke-width="1" />
    <text x={mid[0]} y={mid[1] + 4} text-anchor="middle" font-size="11" font-family="system-ui, sans-serif" fill={TEXT[arrowLook.tone]} style="fill:{TEXT[arrowLook.tone]}">{arrowLook.label}</text>
  {/if}
</svg>
