<!-- The selected box: health, what needs attention, connections, what it is made of and where
     each annotated field came from. All text, never HTML. -->
<script lang="ts">
  import type { Field, MapBox, View } from "./types";

  let { view, box, onclose }: { view: View; box: MapBox; onclose: () => void } = $props();

  const titleOf = (id: string) => view.boxes.find((b) => b.id === id)?.title.value ?? id;
  const mine = $derived(view.arrows.filter((a) => a.from === box.id || a.to === box.id));
  const touched = $derived(new Set([box.id, ...mine.map((a) => a.id)]));
  const health = $derived(view.marks.filter((m) => m.layer === "health" && m.target === box.id));
  const attention = $derived(view.marks.filter((m) => (m.layer === "rules" || m.layer === "risks") && m.state !== "ok" && touched.has(m.target)));
  const source = $derived(view.sources?.[box.id]);
  const parts = $derived(view.parts?.[box.id]);
  const ins = $derived(mine.filter((a) => a.to === box.id));
  const outs = $derived(mine.filter((a) => a.from === box.id));
  const fields = $derived(
    (["kind", "title", "tech", "description", "logo", "badges", "pattern", "slot"] as const)
      .map((k) => [k, box[k] as Field<unknown> | undefined] as const)
      .filter((e): e is readonly [typeof e[0], Field<unknown>] => !!e[1]),
  );
  let card: HTMLElement;
  // focus follows the selection into the card
  $effect(() => {
    void box.id;
    card.focus({ preventScroll: true });
  });
</script>

<section class="details" class:wide={!!source} aria-label="Details" tabindex="-1" bind:this={card}>
  <div class="head">
    <b>{box.title.value}</b>
    <button type="button" aria-label="Close" onclick={onclose}>×</button>
  </div>
  <p class="meta">{[box.kind.value, box.tech?.value, box.level].filter(Boolean).join(" · ")}</p>
  {#if box.description}<p>{box.description.value}</p>{/if}

  {#if health.length}
    <h3>Health</h3>
    <ul>{#each health as m}<li class={m.state}>{m.value ?? m.state}{m.note ? ` — ${m.note}` : ""}</li>{/each}</ul>
  {/if}
  {#if attention.length}
    <h3>Needs attention</h3>
    <ul>{#each attention as m}<li class={m.state}>{m.badge ?? m.value ?? m.state}{m.note ? ` — ${m.note}` : ""}</li>{/each}</ul>
  {/if}
  {#if ins.length || outs.length}
    <h3>Connections</h3>
    <ul>
      {#each ins as a}<li>← {titleOf(a.from)} <span class="muted">{a.label ?? a.kind}{a.count > 1 ? ` ×${a.count}` : ""}</span></li>{/each}
      {#each outs as a}<li>→ {titleOf(a.to)} <span class="muted">{a.label ?? a.kind}{a.count > 1 ? ` ×${a.count}` : ""}</span></li>{/each}
    </ul>
  {/if}
  {#if parts?.length}
    <h3>{box.title.value}</h3>
    <ul>{#each parts as p}<li><span class="muted">{p.kind}</span> <code>{p.name}</code>{p.detail ? ` ${p.detail}` : ""} <span class="muted">:{p.line}</span></li>{/each}</ul>
  {/if}
  {#if source}
    <h3>Code</h3>
    <p class="muted"><code>{source.file}:{source.line}</code> · complexity {source.complexity}</p>
    <pre class="code">{source.text}{source.cut ? `\n… ${source.cut} more lines` : ""}</pre>
  {/if}
  {#if box.madeOf.length}
    <h3>Made of{box.madeOfSource ? ` (${box.madeOfSource})` : ""}</h3>
    <!-- no editor-link helper exists in the viewer: the paths, as text -->
    <ul>{#each box.madeOf as p}<li><code>{p}</code></li>{/each}</ul>
  {/if}
  {#if box.serviceName || box.package}
    <p class="muted">
      {#if box.serviceName}service <code>{box.serviceName}</code>{/if}
      {#if box.package}package <code>{box.package}</code>{/if}
    </p>
  {/if}
  <h3>Sources</h3>
  <ul class="muted">{#each fields as [k, f]}<li>{k}: {f.source}{f.confidence ? `, ${f.confidence}` : ""}</li>{/each}</ul>
</section>

<style>
  .details { position: absolute; top: 56px; right: 16px; width: 300px; max-height: calc(100% - 120px); overflow: auto;
    background: #fffdf8; border: 1px solid #d9d2c3; border-radius: 12px; padding: 12px 14px; font-size: 13px;
    box-shadow: 0 6px 20px rgba(35, 33, 30, 0.12); overflow-wrap: anywhere; }
  .details.wide { width: 480px; }
  .code { margin: 4px 0; padding: 8px; max-height: 360px; overflow: auto; background: #f6f3ec; border-radius: 6px;
    font: 11.5px/15px ui-monospace, Menlo, monospace; white-space: pre; overflow-wrap: normal; }
  .details:focus { outline: none; }
  .details:focus-visible { outline: 3px solid #3b6fd8; outline-offset: 2px; }
  .head { display: flex; align-items: start; gap: 8px; }
  .head b { flex: 1; font-size: 16px; }
  .head button { font: inherit; border: 0; background: none; cursor: pointer; font-size: 18px; line-height: 1; color: #5c5850; }
  h3 { margin: 12px 0 4px; font-size: 11px; text-transform: uppercase; letter-spacing: 0.06em; color: #5c5850; }
  p { margin: 4px 0; }
  li { padding: 1px 0; }
  .meta, .muted { color: #6b6559; }
  .warn { color: #6b4200; } .bad { color: #9b2c1f; } .down { color: #4a4640; } .ok { color: #2f8f5b; }
  @media (max-width: 760px) { .details, .details.wide { left: 16px; right: 16px; width: auto; top: auto; bottom: 56px; max-height: 50%; } }
</style>
