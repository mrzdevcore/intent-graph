<!-- The add-box form (KindAdd artboard), anchored at the click: first the kinds that may sit there, grouped, each
     with its icon and meaning; then the chosen kind's fields (title, its own properties, code). Enter adds, Esc
     closes; the inputs handle Esc themselves because the map's key handler ignores keys while typing. Text only. -->
<script lang="ts">
  import { onMount, tick } from "svelte";
  import type { KindView } from "../types";
  import { NO_CODE, iconMask, prefill, sections } from "./add";

  let { parentTitle, within, kinds, globs, x, y, onadd, onclose }: {
    parentTitle: string; within: boolean; kinds: KindView[]; globs: string[]; x: number; y: number;
    onadd: (a: { kind: string; name: string; at: string[]; props: Record<string, string> }) => void; onclose: () => void;
  } = $props();

  const groups = $derived(sections(kinds, within));
  let kind = $state<KindView | null>(null);
  let name = $state("");
  let values = $state<Record<string, string>>({});
  let typed = $state<string | null>(null); // the code path once edited by hand
  const at = $derived(typed ?? prefill(globs, name));
  const code = $derived(!!kind && !NO_CODE.includes(kind.base));
  let form: HTMLFormElement;
  let field = $state<HTMLInputElement>();
  onMount(() => form.querySelector<HTMLElement>("button[data-kind]")?.focus());

  const lower = (s: string) => s.charAt(0).toLowerCase() + s.slice(1);
  const cap = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);
  async function choose(k: KindView | null) {
    kind = k;
    values = {};
    await tick();
    if (k) field?.focus();
    else form.querySelector<HTMLElement>("button[data-kind]")?.focus();
  }
  function submit(e?: Event) {
    e?.preventDefault();
    if (!kind) return;
    const n = name.trim();
    if (!n) return field?.focus();
    const missing = kind.props.find((p) => p.required && !values[p.name]?.trim());
    if (missing) return form.querySelector<HTMLElement>(`[name="${missing.name}"]`)?.focus();
    const a = at.trim();
    const set = Object.fromEntries(Object.entries(values).map(([k, v]) => [k, v.trim()]).filter(([, v]) => v));
    onadd({ kind: kind.name, name: n, at: code && a ? [a] : [], props: set });
  }
  function keydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      onclose();
    } else if (e.key === "Enter" && (e.target as HTMLElement).tagName === "SELECT") submit(e);
  }
</script>

{#snippet icon(k: KindView)}<span class="ic" aria-hidden="true"><i style:mask-image={iconMask(k.icon)}></i></span>{/snippet}

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<form bind:this={form} data-popover aria-label="New box in {parentTitle}" style:left="{x}px" style:top="{y}px" onsubmit={submit} onkeydown={keydown}>
  {#if !kind}
    <b class="head">{within ? "Add inside" : "Add to"} {parentTitle}</b>
    <div class="list">
      {#each groups as g (g.label)}
        <div role="group" aria-label={g.label}>
          <span class="h">{g.label}</span>
          {#each g.kinds as k (k.name)}
            <button type="button" class="k" data-kind={k.name} onclick={() => choose(k)}>
              {@render icon(k)}
              <span class="t">
                <span class="n">{k.title}{#if k.custom}<span class="base">{` · a ${k.base}`}</span>{/if}</span>
                {#if k.means}<span class="m">{k.means}</span>{/if}
                {#if k.custom}<span class="m">From ig/kinds/{k.name}.kdl</span>{/if}
              </span>
            </button>
          {/each}
        </div>
      {/each}
    </div>
    <div class="row"><button type="button" onclick={onclose}>Cancel <kbd>Esc</kbd></button></div>
  {:else}
    <div class="chosen">
      <button type="button" class="back" aria-label="Other kinds" onclick={() => choose(null)}>‹</button>
      {@render icon(kind)}
      <span class="t"><b>New {kind.title}</b>{#if kind.means}<span class="m">{kind.means}</span>{/if}</span>
    </div>
    <label>Title
      <input bind:this={field} bind:value={name} required autocomplete="off" spellcheck="false" />
    </label>
    {#each kind.props as p (p.name)}
      <label>
        <span>{cap(p.name)}{#if p.means}<span class="means">{` · ${lower(p.means)}`}</span>{/if}{#if p.required}<span class="req">{" · required"}</span>{/if}</span>
        {#if p.oneOf.length}
          <select name={p.name} required={p.required} value={values[p.name] ?? ""} onchange={(e) => (values[p.name] = e.currentTarget.value)}>
            {#if !p.required}<option value=""></option>{/if}
            {#each p.oneOf as o (o)}<option value={o}>{o}</option>{/each}
          </select>
        {:else}
          <input name={p.name} required={p.required} value={values[p.name] ?? ""} oninput={(e) => (values[p.name] = e.currentTarget.value)} autocomplete="off" spellcheck="false" />
        {/if}
      </label>
    {/each}
    {#if code}
      <label>Code (optional)
        <input value={at} oninput={(e) => (typed = e.currentTarget.value)} autocomplete="off" spellcheck="false" />
      </label>
      <small>Filled from the title; change it if the code will live elsewhere.</small>
    {/if}
    <div class="row">
      <button type="button" onclick={onclose}>Cancel <kbd>Esc</kbd></button>
      <button type="submit" class="go">Add <kbd>↵</kbd></button>
    </div>
  {/if}
</form>

<style>
  form { position: absolute; z-index: 5; width: 20rem; max-width: calc(100% - 1rem); max-height: calc(100% - 1rem); overflow: auto; box-sizing: border-box; display: flex; flex-direction: column; gap: var(--space-2-5);
    padding: var(--space-3); background: var(--map-surface); border: 1px solid var(--map-line); border-radius: var(--radius-xl); box-shadow: 0 0.5rem 1.5rem var(--map-shadow); color: var(--map-fg); font-size: var(--text-md); }
  .list { display: flex; flex-direction: column; margin: 0 calc(-1 * var(--space-1-5)); }
  .h { display: block; font-size: var(--text-xs); letter-spacing: 0.05em; text-transform: uppercase; color: var(--map-muted); padding: var(--space-2-5) var(--space-2-5) var(--space-1); }
  .k { display: flex; align-items: center; gap: var(--space-2-5); width: 100%; box-sizing: border-box; padding: var(--space-1-5) var(--space-2-5); border: 0; border-radius: var(--radius-lg);
    background: none; color: var(--map-fg); font: inherit; text-align: left; min-height: 2.5rem; cursor: pointer; }
  .k:hover, .k:focus-visible { background: var(--map-sunken); }
  .ic { width: 1.75rem; height: 1.75rem; border-radius: var(--radius-md); background: var(--map-sunken); display: inline-flex; align-items: center; justify-content: center; flex: none; color: var(--map-fg-2); }
  .ic i { width: 1rem; height: 1rem; background: currentColor; mask-repeat: no-repeat; mask-position: center; mask-size: contain; }
  .t { display: flex; flex-direction: column; min-width: 0; }
  .n { font-weight: 600; font-size: var(--text-md); }
  .base { font-weight: 400; color: var(--map-muted); }
  .m { font-size: var(--text-sm); color: var(--map-muted); }
  .chosen { display: flex; align-items: center; gap: var(--space-2-5); }
  .back { border: 0; background: none; min-height: 2rem; padding: 0 var(--space-1); font-size: var(--text-lg); color: var(--map-muted); }
  label { display: flex; flex-direction: column; gap: var(--space-1); font-size: var(--text-sm); color: var(--map-muted); }
  .req { color: var(--map-warn-text); }
  input, select { font: inherit; font-size: var(--text-md); color: var(--map-fg); background: var(--map-sunken); border: 1px solid var(--map-line); border-radius: var(--radius-md); padding: var(--space-1-5) var(--space-2); min-height: 2rem; }
  small { font-size: var(--text-sm); color: var(--map-faint); }
  .row { display: flex; justify-content: flex-end; gap: var(--space-1-5); }
  button { font: inherit; font-size: var(--text-md); border: 1px solid var(--map-line); background: var(--map-surface); color: var(--map-fg); border-radius: var(--radius-lg); padding: var(--space-1) var(--space-2-5); min-height: 2rem; cursor: pointer; }
  .go { border-color: var(--map-gold); background: var(--map-tint-gold); }
  kbd { font: inherit; font-size: var(--text-sm); color: var(--map-faint); }
  input:focus-visible, select:focus-visible, button:focus-visible { outline: 2px solid var(--map-trace); outline-offset: 2px; }
</style>
