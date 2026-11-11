<!-- A box on the map; port of the mockup's Box.dc.html. Fills its frame; the layout sizes it.
     Open (its inside drawn), it is a frame with a one-line header the inside sits below; zoomed
     into, a function or method with no inside shows its code, and a component's part box its
     list and code (detail "source"). Both keep a steady size on screen: `zoom` is screen pixels
     per unit.
     A run block is drawn as its kind's shape (Shape.svelte). Far: the logo (or the kind's icon) and the title, big;
     mid: + the tech and the status; near: + the description and the chips (status, then at most 2 badges).
     Sizes stay in px: they are the layout's units, scaled with the map. -->
<script lang="ts">
  import type { Look, Tone } from "./looks";
  import type { KindView, MapBox, Part, Snippet, Table as TableData } from "./types";
  import { boxLogo } from "./logo";
  import { steady } from "./camera";
  import { iconMask, kindOf } from "./edit/add";
  import { NO_KIND } from "./edit/kinds";
  import Code from "./Code.svelte";
  import Table from "./Table.svelte";
  import type { TableMark } from "./diff";
  import type { CardFacts } from "./db/card";
  import Shape from "./Shape.svelte";
  import { padOf, hue, shapeOf, type ShapeName } from "./shapes";

  // more: how many boxes it holds
  // parts: a component's part box's list; anatomy: a component's part boxes (title, list), for
  // its chips; w, h: its frame; shape: what it is drawn as (Map knows its children, for a CLI);
  // table: a table box's table, rows: its columns' marks in a diff
  let { box, look, detail, onopen, inside = 0, more = 0, zoom = 1, source, parts, anatomy, kinds = [], w = 264, h = 152, shape: drawnAs, table, rows, facts, picked = false, onfocus }: {
    box: MapBox; look: Look; detail: "far" | "mid" | "near" | "open" | "source"; onopen: () => void;
    inside?: number; more?: number; zoom?: number; source?: Snippet; parts?: Part[];
    anatomy?: { title: string; parts: Part[] }[]; kinds?: KindView[]; w?: number; h?: number; shape?: ShapeName;
    table?: TableData; rows?: TableMark; facts?: CardFacts; picked?: boolean; onfocus?: () => void;
  } = $props();

  const CHIP: Record<Tone, string> = {
    neutral: "background:var(--map-sunken);color:var(--map-fg-2)", warn: "background:var(--map-warn-chip);color:var(--map-warn-text);font-weight:600",
    bad: "background:var(--map-bad-chip);color:var(--map-bad-text);font-weight:600", down: "background:var(--map-fg);color:var(--map-surface);font-weight:600",
  };
  // Lucide-style 24x24 stroke paths, as in the mockup: the code-derived kinds the kinds table does not hold
  const ICON: Record<string, [string, string]> = {
    folder: ["M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.7-.9L9.6 3.9A2 2 0 0 0 7.9 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2z", "var(--map-muted)"],
    file: ["M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7zM14 2v4a2 2 0 0 0 2 2h4", "var(--map-muted)"],
    component: ["M3 5h18v14H3zM3 9h18M7 13h4M7 16h7", "var(--map-blue)"],
    parts: ["M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01", "var(--map-muted)"],
    table: ["M3 5h18v14H3zM3 10h18M9 10v9", "var(--map-blue)"],
  };
  const GLYPH: Record<string, string> = { class: "C", method: "m", group: "≡", task: "▸" };
  // a design block's tile and icon: [tint, ink]
  const TONE: Record<string, [string, string]> = { entry: ["var(--map-tint-blue)", "var(--map-blue)"], usecase: ["var(--map-tint-green)", "var(--map-green)"],
    port: ["var(--map-tint-green)", "var(--map-green)"], domain: ["var(--map-tint-gold)", "var(--map-gold)"] };

  const kind = $derived(box.kind.value);
  const level = $derived(box.level);
  const code = $derived(level === "code");
  const opened = $derived(detail === "open" || detail === "source");
  // an open box's header and code: their size on screen, whatever the zoom
  const m = $derived(opened ? steady(zoom) : 1);
  const far = $derived(detail === "far" || opened);
  const external = $derived(kind === "external");
  const title = $derived(box.title.value);
  const icon = $derived(ICON[kind] ?? null);
  const shape = $derived(drawnAs ?? shapeOf(box));
  // drawn as its shape (a group, a person and an open box keep their frame)
  const drawn = $derived(!opened && !!shape && shape !== "group" && shape !== "person" ? shape : null);
  const brand = $derived(boxLogo(box, drawn ?? undefined));
  const group = $derived(shape === "group");
  // a group's name sits on its dotted top edge, the edge broken under it; steady on screen when open
  const labelSize = $derived(opened ? 12 * m : 16);
  // ponytail: the label's width guessed from its length (0.6em a character, the hand font's average) and its
  // padding: measuring it needs layout, which tests lack
  const gap = $derived<[number, number, number]>([10 * m, (title.length * 0.6 + 1) * labelSize, labelSize * 1.5]);
  // the kind's own icon from the kinds table (a team kind's own), when it has no logo; code has its glyph
  const kindIcon = $derived(code ? null : kindOf(box, kinds)?.icon ?? null);
  // not code, not open: far, mid and near say more and more
  const face = $derived(!code && !opened);
  const status = $derived(look.chips.map((c) => ({ label: c.label, css: CHIP[c.tone] })));
  const badges = $derived(box.badges?.value ?? []);
  const chips = $derived([
    ...status,
    ...badges.slice(0, 2).map((b) => (b[0] === "!" ? { label: b.slice(1), css: CHIP.warn } : { label: b, css: CHIP.neutral })),
    ...(badges.length > 2 ? [{ label: `+${badges.length - 2}`, css: CHIP.neutral }] : []),
    ...count((anatomy ?? []).flatMap((a) => a.parts)).slice(0, 4).map((label) => ({ label, css: CHIP.neutral })),
    ...(inside > 0 && !external && !anatomy?.length ? [{ label: `${inside} inside ›`, css: CHIP.neutral }] : []),
  ]);
  // ["2 props", "1 emit"]
  function count(ps: Part[]) {
    const n = new Map<string, number>();
    for (const p of ps) n.set(p.kind, (n.get(p.kind) ?? 0) + 1);
    return [...n].map(([k, c]) => `${c} ${k}${c > 1 && !/s$/.test(k) ? "s" : ""}`);
  }
  const tally = $derived(count(parts ?? []).join(" · "));
  const meta = $derived(parts ? tally || "none" : [box.tech?.value, code && more ? `${more} inside` : null].filter(Boolean).join(" · "));
  const glow = $derived(look.glow && !drawn ? `0 0 0 5px color-mix(in srgb, ${look.glow} 25%, transparent)` : "none");
  const border = $derived(drawn || group ? "0" : `${(code ? 1 : look.width) * m}px ${look.dashed || external ? "dashed" : "solid"}`);
  // a shape's outline is its kind's hue unless a status (health, build, drift…) says otherwise
  const neutral = $derived(look.edge === "var(--map-edge)");
  const edge = $derived(code && neutral ? "var(--map-line)" : group && neutral ? "var(--map-faint)" : look.edge);
  const pad = $derived(drawn ? padOf(drawn, w, h).map((p) => `${p}px`).join(" ") : opened ? "0" : group ? `${gap[2] + 6}px 14px 12px` : kind === "box" ? "28px 14px 12px" : far ? "10px 12px" : "12px 14px");
  const centred = $derived(face && detail === "far");
  const shown = $derived(detail === "near" ? chips : face && detail === "mid" ? status : []);
  // open, its corners keep their size on screen
  const radius = $derived(external ? "34px" : opened ? `${14 * m}px` : code ? "8px" : "14px 12px 16px 12px");
  const tile = $derived(centred ? 48 : opened ? 18 : code ? 26 : level === "container" || level === "system" ? 36 : 30);
  const glyph = $derived(centred ? 36 : opened ? 14 : 20);
  const titleFont = $derived(code ? "var(--font-mono)" : "var(--font-hand)");
  const titleSize = $derived(centred ? 24 : opened ? 14 : code ? 14 : level === "container" || level === "system" ? 20 : 19);
  const unread = $derived(kind === "group" && box.id.endsWith("/unread"));
</script>

{#if table && kind === "table"}
  <Table {table} {look} {rows} {onopen} {onfocus} {zoom} {h} {facts} {picked} />
{:else if unread}
  <button class="chip-box" type="button" aria-label="Open {title}" onclick={onopen}>{title}</button>
{:else if kind === "person"}
  <button class="figure" type="button" aria-label="Open {title}" style:opacity={look.fade} onclick={onopen}>
    <span class="head" style:border style:border-color={edge} style:box-shadow={glow}>
      <svg width="26" height="26" viewBox="0 0 24 24" fill="none" style:stroke="var(--map-fg-2)" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="8" r="4" /><path d="M4 21c1.5-4 4.5-6 8-6s6.5 2 8 6" /></svg>
    </span>
    <b class="title" style:font-family={titleFont}>{title}</b>
    {#if !far && meta}<span class="meta">{meta}</span>{/if}
  </button>
{:else}
  <button class="card" class:reading={box.needsReading} class:open={opened} class:centred class:cli={drawn === "cli"} type="button" aria-label="Open {title}"
    data-shape={shape} data-logo={brand?.slug}
    style:background={drawn || group ? "none" : look.bg} style:border style:border-color={edge} style:border-radius={radius} style:box-shadow={glow}
    style:padding={pad} onclick={onopen}>
    {#if drawn}<Shape shape={drawn} {w} {h} edge={neutral ? null : look.edge} dashed={look.dashed} width={look.width} glow={look.glow} />{/if}
    {#if group}
      <Shape shape="group" {w} {h} edge={neutral ? null : look.edge} dashed width={look.width * m} glow={null} {gap} />
      <b class="edge-label" style:font-size="{labelSize}px" style:line-height="{gap[2]}px" style:padding="0 {labelSize * 0.5}px" style:left="{gap[0]}px">{title}</b>
    {/if}
    {#if kind === "box"}<span class="band">{NO_KIND}</span>{/if}
    <!-- open, its header and code are drawn at screen size: laid out 1/m as large, scaled by m -->
    <span class="body" class:steady={opened} style:width={opened ? `${100 / m}%` : null} style:height={opened ? `${100 / m}%` : null}
      style:transform={opened ? `scale(${m})` : null}>
    {#if !group}
    <span class="row" style:opacity={look.fade}>
      <span class="tile" style:width="{tile}px" style:height="{tile}px" style:border-radius={external ? "18px" : code ? "6px" : "8px"}
        style:background={drawn ? "none" : level === "component" ? TONE[kind]?.[0] ?? "var(--map-sunken)" : "var(--map-sunken)"}>
        {#if brand}
          <svg class="logo" width={glyph} height={glyph} viewBox="0 0 24 24" aria-hidden="true" style:filter={look.fade < 1 && look.dashed ? "grayscale(1)" : "none"}><path d={brand.path} fill={brand.color} /></svg>
        {:else if kindIcon}
          <i class="kind-icon" data-icon={kindIcon} aria-hidden="true" style:width="{glyph}px" style:height="{glyph}px" style:mask-image={iconMask(kindIcon)} style:background={drawn ? hue(drawn) : TONE[kind]?.[1] ?? "var(--map-fg-2)"}></i>
        {:else if icon}
          <svg width={code ? 16 : glyph} height={code ? 16 : glyph} viewBox="0 0 24 24" fill="none" style:stroke={icon[1]} stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d={icon[0]} /></svg>
        {:else if code}
          <span class="glyph">{GLYPH[kind] ?? "ƒ"}</span>
        {/if}
      </span>
      <span class="names">
        <b class="title" style:font-family={titleFont} style:font-size="{titleSize}px">{title}</b>
        {#if !far && meta}<span class="meta">{meta}</span>{/if}
      </span>
      {#if look.dot}<span class="ok" aria-hidden="true"></span>{/if}
    </span>
    {/if}
    <!-- a list and its code (a template's tags): side by side -->
    <span class="split" class:both={detail === "source" && !!parts?.length && !!source}>
    {#if detail === "source" && parts?.length}
      <span class="parts">{#each parts as p}<span class="pk">{p.kind}</span><b>{p.name}</b><span class="pd">{p.detail ?? ""}</span><span class="pl">{p.line}</span>{/each}</span>
    {/if}
    {#if detail === "source" && source}
      <pre class="code"><Code {source} /></pre>
    {/if}
    </span>
    {#if !far && box.description && (code || detail === "near")}
      <span class="desc" style:opacity={look.fade} style:-webkit-line-clamp={code ? 1 : 2}>{box.description.value}</span>
    {/if}
    {#if shown.length}
      <span class="chips">{#each shown as c}<span style={"padding:2px 8px;border-radius:999px;" + c.css}>{c.label}</span>{/each}</span>
    {/if}
    </span>
  </button>
{/if}

<style>
  button { font: inherit; color: var(--map-fg); text-align: left; cursor: pointer; font-family: var(--font-sans); }
  .card { position: relative; isolation: isolate; width: 100%; height: 100%; box-sizing: border-box; display: flex; flex-direction: column; gap: 8px; overflow: hidden; }
  .band { position: absolute; left: 0; right: 0; top: 0; padding: 4px 10px; font: 600 11px/14px var(--font-sans); color: var(--map-faint); border-bottom: 1px dashed var(--map-line-soft); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .card.reading { outline: 2px dotted var(--map-gold); outline-offset: 3px; }
  /* the focus ring must stay distinct from the needs-reading dots */
  .card:focus-visible, .card.reading:focus-visible, .figure:focus-visible, .chip-box:focus-visible { outline: 3px solid var(--map-blue); outline-offset: 3px; }
  .row { display: flex; align-items: center; gap: 10px; min-width: 0; }
  .tile { flex: none; display: flex; align-items: center; justify-content: center; }
  .glyph { font: 700 15px var(--font-mono); color: var(--map-muted); }
  .names { min-width: 0; display: flex; flex-direction: column; }
  .card.open .title { line-height: 18px; }
  .card.open .glyph { font-size: 11px; }
  .title { line-height: 22px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .meta { font-size: 11px; line-height: 14px; color: var(--map-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .ok { margin-left: auto; flex: none; width: 10px; height: 10px; border-radius: 5px; background: var(--map-green); }
  .desc { font-size: 12.5px; line-height: 17px; color: var(--map-fg-2); display: -webkit-box; -webkit-box-orient: vertical; overflow: hidden; }
  .body { display: contents; }
  .body.steady { position: absolute; left: 0; top: 0; box-sizing: border-box; padding: 6px 12px; display: flex; flex-direction: column;
    gap: 8px; transform-origin: 0 0; overflow: hidden; }
  /* not app.css's dark, scrolling pre */
  .code { flex: 1; min-height: 0; max-height: none; margin: 0; padding: 0; border-radius: 0; background: none; overflow: hidden;
    font: 11px/15px var(--font-mono); color: var(--map-fg); white-space: pre; }
  .parts { display: grid; grid-template-columns: auto auto minmax(0, 1fr) auto; gap: 2px 10px; align-content: start; overflow: hidden;
    font: 11px/15px var(--font-mono); color: var(--map-fg); flex: none; max-height: 60%; }
  .parts b { font-weight: 600; white-space: nowrap; }
  .split { display: contents; }
  .split.both { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 2.4fr); gap: 16px; flex: 1; min-height: 0; }
  .split.both .parts { max-height: none; grid-template-columns: auto minmax(0, 1fr) auto; }
  .split.both .pd { display: none; }
  .centred { justify-content: center; }
  .centred .row { flex-direction: column; gap: 6px; }
  .centred .names { align-items: center; max-width: 100%; }
  .centred .title { line-height: 28px; max-width: 100%; }
  .edge-label { position: absolute; top: 0; font-family: var(--font-hand); color: var(--map-muted); white-space: nowrap; max-width: 80%; overflow: hidden; text-overflow: ellipsis; }
  .kind-icon { display: block; mask-repeat: no-repeat; mask-position: center; mask-size: contain; }
  .card.cli { color: var(--kind-cli-fg); }
  .card.cli .meta, .card.cli .desc { color: var(--kind-cli-muted); }
  .pk { color: var(--map-blue); white-space: nowrap; }
  .pd, .pl { color: var(--map-faint); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .pl { text-align: right; }
  .chips { margin-top: auto; display: flex; flex-wrap: wrap; gap: 6px; font-size: 11px; line-height: 14px; }
  .figure { width: 100%; height: 100%; box-sizing: border-box; padding: 10px; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 6px; text-align: center; background: none; border: 0; }
  .head { width: 52px; height: 52px; border-radius: 26px; background: var(--map-surface); display: flex; align-items: center; justify-content: center; }
  .figure .title { font-size: 20px; line-height: 22px; }
  .chip-box { border: 1px dashed var(--map-faint); border-radius: 999px; background: var(--map-ground); padding: 2px 10px; font-size: 12px; font-weight: 600; }
</style>
