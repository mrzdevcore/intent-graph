<!-- The shape a run block is drawn as, behind its box's text: a service's banded box, an app's window (a CLI's
     terminal), a job's stack, a store's cylinder, a queue's pipe, a model's chip, an agent's capsule, an external's
     dashed box. Same footprint as the box's frame; sizes in the layout's px, scaled with the map. -->
<script module lang="ts">
  import type { MapBox } from "./types";

  export type ShapeName = "service" | "app" | "cli" | "job" | "store" | "queue" | "model" | "agent" | "external" | "person" | "group";
  const DRAWN = new Set(["service", "app", "job", "store", "queue", "model", "agent", "external", "person", "group"]);

  /** What a box is drawn as, by its base kind; an app whose tech names a CLI, or whose entries are commands and
   *  no route, is a terminal. `kids`: its children. None for design blocks and code. */
  export function shapeOf(b: MapBox, kids: MapBox[] = []): ShapeName | undefined {
    const k = b.kind.value;
    if (!DRAWN.has(k)) return undefined;
    if (k !== "app") return k as ShapeName;
    const ways = kids.filter((c) => c.kind.value === "entry").map((c) => c.props?.kind);
    return /\bcli\b/i.test(b.tech?.value ?? "") || (ways.includes("command") && !ways.includes("route")) ? "cli" : "app";
  }

  /** Room the shape keeps clear of the text: top, right, bottom, left. */
  export const PAD: Partial<Record<ShapeName, [number, number, number, number]>> = {
    service: [30, 14, 12, 14], app: [32, 14, 12, 14], cli: [30, 14, 26, 14], job: [12, 50, 24, 14],
    store: [36, 18, 22, 18], queue: [14, 44, 14, 20], model: [20, 22, 20, 22], agent: [14, 44, 14, 26], external: [12, 16, 12, 16],
  };
  // the family whose hue a shape wears (theme tokens --kind-<family>, -fill, -band)
  const FAMILY: Record<string, string> = { model: "ai", agent: "ai", cli: "app" };
  export const hue = (s: ShapeName) => `var(--kind-${FAMILY[s] ?? s})`;
</script>

<script lang="ts">
  let { shape, w, h, edge, dashed, width, glow }: {
    shape: ShapeName; w: number; h: number; edge: string | null; dashed: boolean; width: number; glow: string | null;
  } = $props();

  const fam = $derived(FAMILY[shape] ?? shape);
  const line = $derived(edge ?? `var(--kind-${fam})`);
  const fill = $derived(shape === "cli" ? "var(--kind-cli-bg)" : `var(--kind-${fam}-fill)`);
  const band = $derived(shape === "cli" ? "var(--kind-cli-band)" : `var(--kind-${fam}-band)`);
  const dash = $derived(dashed || shape === "agent" || shape === "external" ? "7 4" : null);
  const s = $derived(width / 2);

  // a rounded rectangle as a path
  const rr = (x: number, y: number, rw: number, rh: number, r: number) =>
    `M${x + r} ${y}H${x + rw - r}A${r} ${r} 0 0 1 ${x + rw} ${y + r}V${y + rh - r}A${r} ${r} 0 0 1 ${x + rw - r} ${y + rh}` +
    `H${x + r}A${r} ${r} 0 0 1 ${x} ${y + rh - r}V${y + r}A${r} ${r} 0 0 1 ${x + r} ${y}Z`;
  // the top band of a rounded rectangle, `bh` high
  const top = (r: number, bh: number) => `M${s} ${bh}V${s + r}A${r} ${r} 0 0 1 ${s + r} ${s}H${w - s - r}A${r} ${r} 0 0 1 ${w - s} ${s + r}V${bh}Z`;
  const ry = $derived(Math.min(14, h / 8)), er = $derived(Math.min(16, w / 10)), pin = 7, cut = 16;
  const card = $derived(shape === "job" ? [w - 12 - 2 * s, h - 12 - 2 * s] : [w - 2 * s, h - 2 * s]);
  const outline = $derived.by(() => {
    switch (shape) {
      case "store": return `M${s} ${ry + s}A${w / 2 - s} ${ry} 0 0 1 ${w - s} ${ry + s}V${h - ry - s}A${w / 2 - s} ${ry} 0 0 1 ${s} ${h - ry - s}Z`;
      case "queue": return `M${er + s} ${s}H${w - er - s}A${er} ${h / 2 - s} 0 0 1 ${w - er - s} ${h - s}H${er + s}A${er} ${h / 2 - s} 0 0 1 ${er + s} ${s}Z`;
      case "model": {
        const [x0, y0, x1, y1] = [pin, pin, w - pin, h - pin];
        return `M${x0 + cut} ${y0}H${x1 - cut}L${x1} ${y0 + cut}V${y1 - cut}L${x1 - cut} ${y1}H${x0 + cut}L${x0} ${y1 - cut}V${y0 + cut}Z`;
      }
      case "agent": return rr(s, s, w - 2 * s, h - 2 * s, Math.min((h - 2 * s) / 2, 36));
      case "external": return rr(s, s, w - 2 * s, h - 2 * s, 14);
      default: return rr(s, s, card[0], card[1], shape === "app" || shape === "cli" ? 10 : 12);
    }
  });
  const pins = $derived([0.3, 0.5, 0.7].map((t) => `M${w * t} 0V${pin}M${w * t} ${h - pin}V${h}M0 ${h * t}H${pin}M${w - pin} ${h * t}H${w}`).join(""));
</script>

<svg class="shape" width={w} height={h} viewBox="0 0 {w} {h}" aria-hidden="true" fill="none" stroke-linecap="round" stroke-linejoin="round">
  {#if glow}<path d={outline} stroke="color-mix(in srgb, {glow} 25%, transparent)" stroke-width={width + 10} />{/if}
  {#if shape === "job"}
    {#each [12, 6] as o}<path d={rr(s + o, s + o, card[0], card[1], 12)} fill={band} stroke={line} stroke-width={width * 0.75} stroke-dasharray={dash} />{/each}
  {/if}
  {#if shape === "model"}<path d={pins} stroke={line} stroke-width={width} />{/if}
  <path d={outline} {fill} />
  {#if shape === "service"}<path d={top(12, 22)} fill={band} />
  {:else if shape === "app" || shape === "cli"}
    <path d={top(10, 24)} fill={band} />
    {#each [14, 24, 34] as x, i}<circle cx={x} cy={13} r={3} fill={shape === "cli" ? "var(--map-faint)" : ["var(--map-bad)", "var(--map-warn)", "var(--map-green)"][i]} />{/each}
    {#if shape === "cli"}<text x={w - 44} y={h - 12} fill="var(--kind-cli-prompt)" font-family="var(--font-mono)" font-size="14">$</text>
      <rect x={w - 30} y={h - 24} width={8} height={14} fill="var(--kind-cli-prompt)" />{/if}
  {:else if shape === "store"}<ellipse cx={w / 2} cy={ry + s} rx={w / 2 - s} ry={ry} fill={band} stroke={line} stroke-width={width} />
  {:else if shape === "queue"}
    <ellipse cx={w - er - s} cy={h / 2} rx={er} ry={h / 2 - s} fill={band} stroke={line} stroke-width={width} />
    <path d={[0, 1, 2].map((i) => `M${w - 2 * er - 48 + i * 12} ${h - 26}l6 7-6 7`).join("")} stroke={line} stroke-width={2} />
  {:else if shape === "job"}
    <circle cx={card[0] - 18} cy={24} r={11} fill={band} stroke={line} stroke-width={2} />
    <path d="M{card[0] - 18} 18V24L{card[0] - 13} 27" stroke={line} stroke-width={2} />
  {:else if shape === "agent"}
    <path d="M{w - 22} {h / 2 - 2}a10 10 0 1 1-3-7M{w - 27} {h / 2 - 14}l3 5-6 1" stroke={line} stroke-width={2} />
  {/if}
  <path d={outline} stroke={line} stroke-width={width} stroke-dasharray={dash} />
</svg>

<style>
  .shape { position: absolute; inset: 0; overflow: visible; pointer-events: none; z-index: -1; }
</style>
