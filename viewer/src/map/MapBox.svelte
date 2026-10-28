<!-- A box on the map; port of the mockup's Box.dc.html. Fills its frame; the layout sizes it.
     Fonts: Kalam is not bundled (ig view is offline), so titles fall back to a handwriting stack. -->
<script lang="ts">
  import type { Look, Tone } from "./looks";
  import type { MapBox } from "./types";
  import { logo } from "./logo";

  let { box, look, detail, onopen, inside = 0 }: {
    box: MapBox; look: Look; detail: "far" | "mid" | "near"; onopen: () => void; inside?: number;
  } = $props();

  const CHIP: Record<Tone, string> = {
    neutral: "background:#f2eee5;color:#4a4640", warn: "background:#fdf0d5;color:#6b4200;font-weight:600",
    bad: "background:#fbe4e1;color:#9b2c1f;font-weight:600", down: "background:#23211e;color:#fff;font-weight:600",
  };
  // Lucide-style 24x24 stroke paths, as in the mockup.
  const ICON: Record<string, [string, string]> = {
    system: ["M3 3h7v7H3zM14 3h7v7h-7zM3 14h7v7H3zM14 14h7v7h-7z", "#3a3631"],
    app: ["M3 4h18v13H3zM8 21h8M12 17v4", "#3a3631"], entry: ["M3 4h18v13H3zM8 21h8M12 17v4", "#3b6fd8"],
    api: ["M3 4h18v7H3zM3 13h18v7H3zM7 7.5h.01M7 16.5h.01", "#3a3631"],
    worker: ["M12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6zM12 2v3M12 19v3M2 12h3M19 12h3M4.9 4.9l2.1 2.1M17 17l2.1 2.1M4.9 19.1L7 17M17 7l2.1-2.1", "#3a3631"],
    store: ["M4 5c0-1.7 3.6-3 8-3s8 1.3 8 3-3.6 3-8 3-8-1.3-8-3zM4 5v14c0 1.7 3.6 3 8 3s8-1.3 8-3V5M4 12c0 1.7 3.6 3 8 3s8-1.3 8-3", "#3a3631"],
    queue: ["M3 6h18M3 12h18M3 18h12", "#3a3631"],
    external: ["M7 18a5 5 0 1 1 1-9.9A6 6 0 0 1 19 10a4 4 0 0 1-1 8z", "#5c5850"],
    usecase: ["M5 12h14M13 6l6 6-6 6", "#2f8f5b"], port: ["M9 2v6M15 2v6M6 8h12v4a6 6 0 0 1-12 0zM12 18v4", "#2f8f5b"],
    domain: ["M12 2l9 5v10l-9 5-9-5V7zM3 7l9 5 9-5M12 12v10", "#9a6b00"],
  };
  const TINT: Record<string, string> = { entry: "#e6eefc", usecase: "#e3f3ea", port: "#e3f3ea", domain: "#fbefc8" };

  const kind = $derived(box.kind.value);
  const level = $derived(box.level);
  const code = $derived(level === "code");
  const far = $derived(detail === "far");
  const external = $derived(kind === "external");
  const title = $derived(box.title.value);
  const brand = $derived(box.logo ? logo(box.logo.value) : null);
  const icon = $derived(code ? null : ICON[kind] ?? null);
  const chips = $derived([
    ...look.chips.map((c) => ({ label: c.label, css: CHIP[c.tone] })),
    ...(box.badges?.value ?? []).map((b) => (b[0] === "!" ? { label: b.slice(1), css: CHIP.warn } : { label: b, css: CHIP.neutral })),
    ...(inside > 0 && !external ? [{ label: `${inside} inside ›`, css: CHIP.neutral }] : []),
  ]);
  const meta = $derived([kind, box.tech?.value].filter(Boolean).join(" · "));
  const glow = $derived(look.glow ? `0 0 0 5px color-mix(in srgb, ${look.glow} 25%, transparent)` : "none");
  const border = $derived(`${code ? 1 : look.width}px ${look.dashed || external ? "dashed" : "solid"} ${code && look.edge === "#a39b8e" ? "#c9c2b4" : look.edge}`);
  const radius = $derived(external ? "34px" : code ? "8px" : "14px 12px 16px 12px");
  const tile = $derived(code ? 26 : level === "container" || level === "system" ? 36 : 30);
  const titleFont = $derived(code ? "ui-monospace, Menlo, monospace" : 'Kalam, "Comic Sans MS", cursive');
  const titleSize = $derived(code ? 14 : level === "container" || level === "system" ? 20 : 19);
  const unread = $derived(kind === "group" && box.id.endsWith("/unread"));
</script>

{#if unread}
  <button class="chip-box" type="button" aria-label="Open {title}" onclick={onopen}>{title}</button>
{:else if kind === "person"}
  <button class="figure" type="button" aria-label="Open {title}" style:opacity={look.fade} onclick={onopen}>
    <span class="head" style:border style:box-shadow={glow}>
      <svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="#3a3631" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="8" r="4" /><path d="M4 21c1.5-4 4.5-6 8-6s6.5 2 8 6" /></svg>
    </span>
    <b class="title" style:font-family={titleFont}>{title}</b>
    {#if !far && meta}<span class="meta">{meta}</span>{/if}
  </button>
{:else}
  <button class="card" class:reading={box.needsReading} type="button" aria-label="Open {title}"
    style:background={look.bg} style:border style:border-radius={radius} style:box-shadow={glow}
    style:padding={far ? "10px 12px" : "12px 14px"} onclick={onopen}>
    <span class="row" style:opacity={look.fade}>
      <span class="tile" style:width="{tile}px" style:height="{tile}px" style:border-radius={external ? "18px" : code ? "6px" : "8px"}
        style:background={level === "component" ? TINT[kind] ?? "#f2eee5" : "#f2eee5"}>
        {#if brand}
          <svg width="20" height="20" viewBox="0 0 24 24" aria-hidden="true" style:filter={look.fade < 1 && look.dashed ? "grayscale(1)" : "none"}><path d={brand.path} fill={brand.color} /></svg>
        {:else if code}
          <span class="glyph">{kind === "class" ? "C" : "ƒ"}</span>
        {:else if icon}
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke={icon[1]} stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d={icon[0]} /></svg>
        {/if}
      </span>
      <span class="names">
        <b class="title" style:font-family={titleFont} style:font-size="{titleSize}px">{title}</b>
        {#if !far && meta}<span class="meta">{meta}</span>{/if}
      </span>
      {#if look.dot}<span class="ok" aria-hidden="true"></span>{/if}
    </span>
    {#if !far && box.description}
      <span class="desc" style:opacity={look.fade} style:-webkit-line-clamp={code ? 1 : 2}>{box.description.value}</span>
    {/if}
    {#if detail === "near" && chips.length}
      <span class="chips">{#each chips as c}<span style={"padding:2px 8px;border-radius:999px;" + c.css}>{c.label}</span>{/each}</span>
    {/if}
  </button>
{/if}

<style>
  button { font: inherit; color: #23211e; text-align: left; cursor: pointer; font-family: system-ui, sans-serif; }
  .card { width: 100%; height: 100%; box-sizing: border-box; display: flex; flex-direction: column; gap: 8px; overflow: hidden; }
  .card.reading { outline: 2px dotted #b8860b; outline-offset: 3px; }
  /* the focus ring must stay distinct from the needs-reading dots */
  .card:focus-visible, .card.reading:focus-visible, .figure:focus-visible, .chip-box:focus-visible { outline: 3px solid #3b6fd8; outline-offset: 3px; }
  .row { display: flex; align-items: center; gap: 10px; min-width: 0; }
  .tile { flex: none; display: flex; align-items: center; justify-content: center; }
  .glyph { font: 700 15px ui-monospace, Menlo, monospace; color: #5c5850; }
  .names { min-width: 0; display: flex; flex-direction: column; }
  .title { line-height: 22px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .meta { font-size: 11px; line-height: 14px; color: #5c5850; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .ok { margin-left: auto; flex: none; width: 10px; height: 10px; border-radius: 5px; background: #2f8f5b; }
  .desc { font-size: 12.5px; line-height: 17px; color: #4a4640; display: -webkit-box; -webkit-box-orient: vertical; overflow: hidden; }
  .chips { margin-top: auto; display: flex; flex-wrap: wrap; gap: 6px; font-size: 11px; line-height: 14px; }
  .figure { width: 100%; height: 100%; box-sizing: border-box; padding: 10px; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 6px; text-align: center; background: none; border: 0; }
  .head { width: 52px; height: 52px; border-radius: 26px; background: #fff; display: flex; align-items: center; justify-content: center; }
  .figure .title { font-size: 20px; line-height: 22px; }
  .chip-box { border: 1px dashed #8a8378; border-radius: 999px; background: #faf8f3; padding: 2px 10px; font-size: 12px; font-weight: 600; }
</style>
