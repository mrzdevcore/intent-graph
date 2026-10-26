<!-- The feature switcher: the project's features (plans/<slug>/), and a new one from a title.
     A feature without a design (a flat plan) is listed but can't be opened on the board. -->
<script lang="ts">
  import type { DesignApi, Feature } from "../api";

  let { api, slug, onpick }: { api: DesignApi; slug: string | null; onpick: (slug: string) => void } = $props();

  let features = $state.raw<Feature[]>([]);
  let adding = $state(false);
  let error = $state("");

  const msg = (e: unknown) => String((e as Error)?.message ?? e);

  export async function refresh() {
    try {
      features = await api.features();
      error = "";
    } catch (e) {
      error = msg(e);
    }
    return features;
  }

  async function create(title: string) {
    adding = false;
    if (!title.trim()) return;
    try {
      const f = await api.create(title.trim());
      await refresh();
      onpick(f.slug);
    } catch (e) {
      error = msg(e);
    }
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Enter") create((e.currentTarget as HTMLInputElement).value);
    else if (e.key === "Escape") adding = false;
  }

  $effect(() => void refresh());
</script>

<div class="features">
  <select aria-label="Feature" value={slug ?? ""} onchange={(e) => onpick(e.currentTarget.value)}>
    {#if !slug}<option value="" disabled>Pick a feature…</option>{/if}
    {#each features as f (f.slug)}
      <option value={f.slug} disabled={!f.design}>{f.title}{f.done ? " (done)" : ""}{f.design ? "" : " (plan only)"}</option>
    {/each}
  </select>
  {#if adding}
    <!-- svelte-ignore a11y_autofocus -->
    <input class="title" placeholder="Feature title, Enter to create" aria-label="New feature title" autofocus {onkeydown}
      onblur={() => (adding = false)}>
  {:else}
    <button onclick={() => (adding = true)} title="A new feature: plans/<slug>/design.json">New feature</button>
  {/if}
  {#if error}<span class="error" role="alert">{error}</span>{/if}
</div>

<style>
  .features { display: flex; gap: 6px; align-items: center; min-width: 0; }
  select { max-width: 16em; }
  .title { width: 16em; }
  .error { color: var(--error); font-size: 12px; }
</style>
