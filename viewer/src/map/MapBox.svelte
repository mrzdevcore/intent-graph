<!-- A box on the map; port of the mockup's Box.dc.html. Fills its frame; the layout sizes it.
     Open (its inside drawn), it is a frame with a one-line header the inside sits below; zoomed
     into, a function or method with no inside shows its code, and a component's part box its
     list and code (detail "source"). Both keep a steady size on screen: `zoom` is screen pixels
     per unit.
     Sizes stay in px: they are the layout's units, scaled with the map. -->
<script lang="ts">
  import type { Look, Tone } from "./looks";
  import type { MapBox, Part, Snippet } from "./types";
  import { logo } from "./logo";
  import { steady } from "./camera";
  import Code from "./Code.svelte";

  // more: how many boxes it holds
  // parts: a component's part box's list; anatomy: a component's part boxes (title, list), for
  // its card's summary
  let { box, look, detail, onopen, inside = 0, more = 0, zoom = 1, source, parts, anatomy }: {
    box: MapBox; look: Look; detail: "far" | "mid" | "near" | "open" | "source"; onopen: () => void;
    inside?: number; more?: number; zoom?: number; source?: Snippet; parts?: Part[];
    anatomy?: { title: string; parts: Part[] }[];
  } = $props();

  const CHIP: Record<Tone, string> = {
    neutral: "background:var(--map-sunken);color:var(--map-fg-2)", warn: "background:var(--map-warn-chip);color:var(--map-warn-text);font-weight:600",
    bad: "background:var(--map-bad-chip);color:var(--map-bad-text);font-weight:600", down: "background:var(--map-fg);color:var(--map-surface);font-weight:600",
  };
  // Lucide-style 24x24 stroke paths, as in the mockup.
  const ICON: Record<string, [string, string]> = {
    system: ["M3 3h7v7H3zM14 3h7v7h-7zM3 14h7v7H3zM14 14h7v7h-7z", "var(--map-fg-2)"],
    app: ["M3 4h18v13H3zM8 21h8M12 17v4", "var(--map-fg-2)"], entry: ["M3 4h18v13H3zM8 21h8M12 17v4", "var(--map-blue)"],
    api: ["M3 4h18v7H3zM3 13h18v7H3zM7 7.5h.01M7 16.5h.01", "var(--map-fg-2)"],
    worker: ["M12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6zM12 2v3M12 19v3M2 12h3M19 12h3M4.9 4.9l2.1 2.1M17 17l2.1 2.1M4.9 19.1L7 17M17 7l2.1-2.1", "var(--map-fg-2)"],
    store: ["M4 5c0-1.7 3.6-3 8-3s8 1.3 8 3-3.6 3-8 3-8-1.3-8-3zM4 5v14c0 1.7 3.6 3 8 3s8-1.3 8-3V5M4 12c0 1.7 3.6 3 8 3s8-1.3 8-3", "var(--map-fg-2)"],
    queue: ["M3 6h18M3 12h18M3 18h12", "var(--map-fg-2)"],
    external: ["M7 18a5 5 0 1 1 1-9.9A6 6 0 0 1 19 10a4 4 0 0 1-1 8z", "var(--map-muted)"],
    usecase: ["M5 12h14M13 6l6 6-6 6", "var(--map-green)"], port: ["M9 2v6M15 2v6M6 8h12v4a6 6 0 0 1-12 0zM12 18v4", "var(--map-green)"],
    domain: ["M12 2l9 5v10l-9 5-9-5V7zM3 7l9 5 9-5M12 12v10", "var(--map-gold)"],
    folder: ["M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.7-.9L9.6 3.9A2 2 0 0 0 7.9 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2z", "var(--map-muted)"],
    file: ["M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7zM14 2v4a2 2 0 0 0 2 2h4", "var(--map-muted)"],
    component: ["M3 5h18v14H3zM3 9h18M7 13h4M7 16h7", "var(--map-blue)"],
    parts: ["M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01", "var(--map-muted)"],
  };
  const GLYPH: Record<string, string> = { class: "C", method: "m", group: "≡", task: "▸" };
  const TINT: Record<string, string> = { entry: "var(--map-tint-blue)", usecase: "var(--map-tint-green)", port: "var(--map-tint-green)", domain: "var(--map-tint-gold)" };

  const kind = $derived(box.kind.value);
  const level = $derived(box.level);
  const code = $derived(level === "code");
  const opened = $derived(detail === "open" || detail === "source");
  // an open box's header and code: their size on screen, whatever the zoom
  const m = $derived(opened ? steady(zoom) : 1);
  const far = $derived(detail === "far" || opened);
  const external = $derived(kind === "external");
  const title = $derived(box.title.value);
  const brand = $derived(box.logo ? logo(box.logo.value) : null);
  const icon = $derived(ICON[kind] ?? null);
  const chips = $derived([
    ...look.chips.map((c) => ({ label: c.label, css: CHIP[c.tone] })),
    ...(box.badges?.value ?? []).map((b) => (b[0] === "!" ? { label: b.slice(1), css: CHIP.warn } : { label: b, css: CHIP.neutral })),
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
  // a component's card: what it takes, keeps, uses and renders, a line each
  const VERB: Record<string, string> = { Inputs: "takes", State: "keeps", Uses: "uses", Template: "renders", Renders: "renders" };
  const summary = $derived((anatomy ?? []).filter((a) => a.parts.length)
    .map((a) => ({ verb: VERB[a.title] ?? a.title.toLowerCase(), names: [...new Set(a.parts.map((p) => p.name))].join(", ") })));
  const meta = $derived(parts ? tally || "none" : [kind, box.tech?.value, code && more ? `${more} inside` : null].filter(Boolean).join(" · "));
  const glow = $derived(look.glow ? `0 0 0 5px color-mix(in srgb, ${look.glow} 25%, transparent)` : "none");
  const border = $derived(`${(code ? 1 : look.width) * m}px ${look.dashed || external ? "dashed" : "solid"}`);
  const edge = $derived(code && look.edge === "var(--map-edge)" ? "var(--map-line)" : look.edge);
  // open, its corners keep their size on screen
  const radius = $derived(external ? "34px" : opened ? `${14 * m}px` : code ? "8px" : "14px 12px 16px 12px");
  const tile = $derived(opened ? 18 : code ? 26 : level === "container" || level === "system" ? 36 : 30);
  const titleFont = $derived(code ? "var(--font-mono)" : "var(--font-hand)");
  const titleSize = $derived(opened ? 14 : code ? 14 : level === "container" || level === "system" ? 20 : 19);
  const unread = $derived(kind === "group" && box.id.endsWith("/unread"));
</script>

{#if unread}
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
  <button class="card" class:reading={box.needsReading} class:open={opened} type="button" aria-label="Open {title}"
    style:background={look.bg} style:border style:border-color={edge} style:border-radius={radius} style:box-shadow={glow}
    style:padding={opened ? "0" : far ? "10px 12px" : "12px 14px"} onclick={onopen}>
    <!-- open, its header and code are drawn at screen size: laid out 1/m as large, scaled by m -->
    <span class="body" class:steady={opened} style:width={opened ? `${100 / m}%` : null} style:height={opened ? `${100 / m}%` : null}
      style:transform={opened ? `scale(${m})` : null}>
    <span class="row" style:opacity={look.fade}>
      <span class="tile" style:width="{tile}px" style:height="{tile}px" style:border-radius={external ? "18px" : code ? "6px" : "8px"}
        style:background={level === "component" ? TINT[kind] ?? "var(--map-sunken)" : "var(--map-sunken)"}>
        {#if brand}
          <svg width={opened ? 14 : 20} height={opened ? 14 : 20} viewBox="0 0 24 24" aria-hidden="true" style:filter={look.fade < 1 && look.dashed ? "grayscale(1)" : "none"}><path d={brand.path} fill={brand.color} /></svg>
        {:else if icon}
          <svg width={opened ? 14 : code ? 16 : 20} height={opened ? 14 : code ? 16 : 20} viewBox="0 0 24 24" fill="none" style:stroke={icon[1]} stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d={icon[0]} /></svg>
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
    <!-- a list and its code (a template's tags): side by side -->
    <span class="split" class:both={detail === "source" && !!parts?.length && !!source}>
    {#if detail === "source" && parts?.length}
      <span class="parts">{#each parts as p}<span class="pk">{p.kind}</span><b>{p.name}</b><span class="pd">{p.detail ?? ""}</span><span class="pl">{p.line}</span>{/each}</span>
    {/if}
    {#if detail === "source" && source}
      <pre class="code"><Code {source} /></pre>
    {/if}
    </span>
    {#if !far && box.description}
      <span class="desc" style:opacity={look.fade} style:-webkit-line-clamp={code ? 1 : 2}>{box.description.value}</span>
    {/if}
    {#if detail === "near" && summary.length && !box.description}
      <span class="summary" style:opacity={look.fade}>{#each summary as s}<span><span class="verb">{s.verb}</span> {s.names}</span>{/each}</span>
    {/if}
    {#if detail === "near" && chips.length}
      <span class="chips">{#each chips as c}<span style={"padding:2px 8px;border-radius:999px;" + c.css}>{c.label}</span>{/each}</span>
    {/if}
    </span>
  </button>
{/if}

<style>
  button { font: inherit; color: var(--map-fg); text-align: left; cursor: pointer; font-family: var(--font-sans); }
  .card { position: relative; width: 100%; height: 100%; box-sizing: border-box; display: flex; flex-direction: column; gap: 8px; overflow: hidden; }
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
  .summary { display: flex; flex-direction: column; gap: 2px; min-height: 0; overflow: hidden; font: 11.5px/16px var(--font-mono); color: var(--map-muted); }
  .summary > span { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .verb { color: var(--map-blue); }
  .pk { color: var(--map-blue); white-space: nowrap; }
  .pd, .pl { color: var(--map-faint); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .pl { text-align: right; }
  .chips { margin-top: auto; display: flex; flex-wrap: wrap; gap: 6px; font-size: 11px; line-height: 14px; }
  .figure { width: 100%; height: 100%; box-sizing: border-box; padding: 10px; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 6px; text-align: center; background: none; border: 0; }
  .head { width: 52px; height: 52px; border-radius: 26px; background: var(--map-surface); display: flex; align-items: center; justify-content: center; }
  .figure .title { font-size: 20px; line-height: 22px; }
  .chip-box { border: 1px dashed var(--map-faint); border-radius: 999px; background: var(--map-ground); padding: 2px 10px; font-size: 12px; font-weight: 600; }
</style>
